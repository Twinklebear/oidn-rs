//! Interop round trip between Direct3D 12 and Open Image Denoise.
//!
//! Direct3D 12 uploads a noisy image into a shared buffer and signals a shared
//! fence, Open Image Denoise waits on the fence it imported as a semaphore,
//! denoises into a second shared buffer and signals the fence back, and
//! Direct3D 12 copies the result out. Nothing is read back through Open Image
//! Denoise, so the test fails unless both APIs really are looking at the same
//! memory and the synchronization holds.
//!
//! Importing external memory and semaphores is supported only by CUDA and HIP
//! devices, so this needs a supported GPU and is not part of the default test
//! run:
//!
//! ```text
//! cargo test --features d3d12-interop --test d3d12_interop -- --nocapture
//! ```

#![cfg(all(windows, feature = "d3d12-interop"))]

use std::ffi::c_void;

use oidn::{ExternalMemoryTypeFlags, ExternalSemaphoreTypeFlags};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::{Interface, PCWSTR};

const WIDTH: usize = 64;
const HEIGHT: usize = 64;
const PIXELS: usize = WIDTH * HEIGHT * 3;
const BYTE_SIZE: usize = PIXELS * size_of::<f32>();

/// `GENERIC_ALL`, the access mask Direct3D 12 shared handles are created with.
const GENERIC_ALL: u32 = 0x1000_0000;

/// How long to wait for the round trip before declaring it hung, rather than
/// blocking the test run forever if a signal never arrives.
const TIMEOUT_MS: u32 = 30_000;

#[test]
fn d3d12_shared_buffer_and_fence_round_trip() {
    let Some(d3d12) = D3d12::new() else {
        eprintln!("skipped: no Direct3D 12 device");
        return;
    };

    let device = match oidn::Device::by_luid(&d3d12.adapter_luid()) {
        Ok(device) => device,
        Err((err, msg)) => {
            eprintln!("skipped: no OIDN device on the Direct3D 12 adapter: {err:?}: {msg}");
            return;
        }
    };

    let memory_types = device.external_memory_types();
    let semaphore_types = device.external_semaphore_types();
    println!("external memory types: {memory_types:?}");
    println!("external semaphore types: {semaphore_types:?}");

    if !memory_types.contains(ExternalMemoryTypeFlags::D3D12_RESOURCE)
        || !semaphore_types.contains(ExternalSemaphoreTypeFlags::D3D12_FENCE)
    {
        eprintln!("skipped: device cannot import Direct3D 12 resources or fences");
        return;
    }

    let (noisy, clean) = test_image();

    // Memory and synchronization shared between the two APIs.
    let input = d3d12.shared_buffer(D3D12_HEAP_TYPE_DEFAULT, BYTE_SIZE);
    let output = d3d12.shared_buffer(D3D12_HEAP_TYPE_DEFAULT, BYTE_SIZE);
    let fence = d3d12.shared_fence();

    let shared_input = import_buffer(&device, &d3d12, &input);
    let shared_output = import_buffer(&device, &d3d12, &output);
    let semaphore = import_fence(&device, &d3d12, &fence);

    // Direct3D 12: upload the noisy image, then signal that it is ready.
    d3d12.upload(&input, &noisy);
    unsafe { d3d12.queue.Signal(&fence, 1) }.expect("signalling the upload should succeed");

    // Open Image Denoise: wait for the upload, denoise, signal completion.
    unsafe { device.wait_semaphores_async(&[&semaphore], Some(&[1]), None) }
        .expect("waiting for the upload fence value should be accepted");

    let mut filter = oidn::RayTracing::try_new(&device).expect("the RT filter should be created");
    filter.srgb(false).image_dimensions(WIDTH, HEIGHT);
    filter
        .filter_buffer(&shared_input, &shared_output)
        .expect("denoising the shared buffers should succeed");

    unsafe { device.signal_semaphores_async(&[&semaphore], Some(&[2])) }
        .expect("signalling the denoised fence value should be accepted");
    device.sync();

    // Direct3D 12: wait for the denoised image, then copy it back to the host.
    unsafe { d3d12.queue.Wait(&fence, 2) }.expect("waiting on the denoised fence value");
    let denoised = d3d12.download(&output, &fence, 3);

    assert!(
        denoised.iter().all(|value| value.is_finite()),
        "denoised image should not contain NaNs or infinities"
    );
    assert_ne!(
        denoised, noisy,
        "the denoised image should differ from the input, \
         which it cannot if the buffers are not really shared"
    );

    let before = mean_squared_error(&noisy, &clean);
    let after = mean_squared_error(&denoised, &clean);
    println!("mean squared error: {before:.6} before, {after:.6} after");
    assert!(
        after < before,
        "denoising should move the image closer to the clean reference"
    );
}

fn import_buffer(device: &oidn::Device, d3d12: &D3d12, resource: &ID3D12Resource) -> oidn::Buffer {
    let handle = d3d12.shared_handle(resource);

    // A committed resource owns its allocation, so it has to be imported as a
    // dedicated one.
    let buffer = unsafe {
        device.create_shared_buffer_from_win32_handle(
            ExternalMemoryTypeFlags::D3D12_RESOURCE | ExternalMemoryTypeFlags::DEDICATED,
            handle.0,
            None,
            BYTE_SIZE,
        )
    };

    // The import does not consume an NT handle.
    unsafe { CloseHandle(handle) }.expect("closing the shared resource handle");

    buffer.expect("importing the shared Direct3D 12 buffer should succeed")
}

fn import_fence(device: &oidn::Device, d3d12: &D3d12, fence: &ID3D12Fence) -> oidn::Semaphore {
    let handle = d3d12.shared_handle(fence);

    let semaphore = unsafe {
        device.create_shared_semaphore_from_win32_handle(
            ExternalSemaphoreTypeFlags::D3D12_FENCE,
            handle.0,
            None,
        )
    };

    unsafe { CloseHandle(handle) }.expect("closing the shared fence handle");

    semaphore.expect("importing the shared Direct3D 12 fence should succeed")
}

/// A smooth image and the same image with reproducible noise added, so that
/// denoising has something to remove and the result can be scored against the
/// original.
fn test_image() -> (Vec<f32>, Vec<f32>) {
    let mut clean = vec![0.0f32; PIXELS];
    let mut noisy = vec![0.0f32; PIXELS];

    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut noise = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        ((state >> 40) as f32 / 16777216.0 - 0.5) * 0.4
    };

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let idx = (y * WIDTH + x) * 3;
            let gradient = [
                x as f32 / WIDTH as f32,
                y as f32 / HEIGHT as f32,
                ((x + y) as f32 / (WIDTH + HEIGHT) as f32).powi(2),
            ];

            for channel in 0..3 {
                clean[idx + channel] = gradient[channel];
                noisy[idx + channel] = (gradient[channel] + noise()).clamp(0.0, 1.0);
            }
        }
    }

    (noisy, clean)
}

fn mean_squared_error(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(a, b)| (a - b) * (a - b)).sum::<f32>() / a.len() as f32
}

/// The Direct3D 12 side of the test: a device, a queue, and the command list
/// used for the uploads and downloads.
struct D3d12 {
    device: ID3D12Device,
    queue: ID3D12CommandQueue,
    allocator: ID3D12CommandAllocator,
    list: ID3D12GraphicsCommandList,
}

impl D3d12 {
    fn new() -> Option<Self> {
        let mut device: Option<ID3D12Device> = None;
        unsafe { D3D12CreateDevice(None, D3D_FEATURE_LEVEL_11_0, &mut device) }.ok()?;
        let device = device?;

        let queue: ID3D12CommandQueue = unsafe {
            device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                ..Default::default()
            })
        }
        .ok()?;

        let allocator: ID3D12CommandAllocator =
            unsafe { device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }.ok()?;

        let list: ID3D12GraphicsCommandList = unsafe {
            device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
        }
        .ok()?;
        unsafe { list.Close() }.ok()?;

        Some(Self {
            device,
            queue,
            allocator,
            list,
        })
    }

    fn adapter_luid(&self) -> [u8; 8] {
        let luid = unsafe { self.device.GetAdapterLuid() };

        let mut bytes = [0u8; 8];
        bytes[..4].copy_from_slice(&luid.LowPart.to_ne_bytes());
        bytes[4..].copy_from_slice(&luid.HighPart.to_ne_bytes());
        bytes
    }

    /// Creates a buffer resource. Buffers on the default heap are created as
    /// shareable, so that Open Image Denoise can import them.
    fn shared_buffer(&self, heap_type: D3D12_HEAP_TYPE, byte_size: usize) -> ID3D12Resource {
        let (state, flags) = match heap_type {
            D3D12_HEAP_TYPE_UPLOAD => (D3D12_RESOURCE_STATE_GENERIC_READ, D3D12_HEAP_FLAG_NONE),
            D3D12_HEAP_TYPE_READBACK => (D3D12_RESOURCE_STATE_COPY_DEST, D3D12_HEAP_FLAG_NONE),
            _ => (D3D12_RESOURCE_STATE_COMMON, D3D12_HEAP_FLAG_SHARED),
        };

        let properties = D3D12_HEAP_PROPERTIES {
            Type: heap_type,
            ..Default::default()
        };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: 0,
            Width: byte_size as u64,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: D3D12_RESOURCE_FLAG_NONE,
        };

        let mut resource: Option<ID3D12Resource> = None;
        unsafe {
            self.device.CreateCommittedResource(
                &properties,
                flags,
                &desc,
                state,
                None,
                &mut resource,
            )
        }
        .expect("creating a buffer resource should succeed");

        resource.expect("a created resource should not be null")
    }

    fn shared_fence(&self) -> ID3D12Fence {
        unsafe { self.device.CreateFence(0, D3D12_FENCE_FLAG_SHARED) }
            .expect("creating a shared fence should succeed")
    }

    fn shared_handle<I: Interface>(&self, object: &I) -> HANDLE {
        self.named_shared_handle(object, PCWSTR::null())
    }

    /// Publishes an object under `name`, returning the handle that keeps the
    /// name registered. Another process can then open the same object by name.
    fn publish<I: Interface>(&self, object: &I, name: &str) -> HANDLE {
        let name = wide(name);
        self.named_shared_handle(object, PCWSTR(name.as_ptr()))
    }

    fn named_shared_handle<I: Interface>(&self, object: &I, name: PCWSTR) -> HANDLE {
        unsafe {
            self.device.CreateSharedHandle(
                &object.cast::<ID3D12DeviceChild>().unwrap(),
                None,
                GENERIC_ALL,
                name,
            )
        }
        .expect("creating a shared handle should succeed")
    }

    /// Copies `contents` into `destination` and waits for the copy to finish.
    fn upload(&self, destination: &ID3D12Resource, contents: &[f32]) {
        let byte_size = size_of_val(contents);
        let staging = self.shared_buffer(D3D12_HEAP_TYPE_UPLOAD, byte_size);

        unsafe {
            let mut mapped: *mut c_void = std::ptr::null_mut();
            staging
                .Map(0, None, Some(&mut mapped))
                .expect("mapping the upload buffer should succeed");
            std::ptr::copy_nonoverlapping(contents.as_ptr(), mapped.cast::<f32>(), contents.len());
            staging.Unmap(0, None);
        }

        self.record(|list| unsafe {
            list.CopyBufferRegion(destination, 0, &staging, 0, byte_size as u64);
        });

        let fence = unsafe { self.device.CreateFence(0, D3D12_FENCE_FLAG_NONE) }
            .expect("creating a fence should succeed");
        self.wait_for(&fence, 1);
    }

    /// Copies `source` back to the host, waiting for `fence` to reach `value`
    /// first. The copy is queued behind whatever the caller already made the
    /// queue wait on.
    fn download(&self, source: &ID3D12Resource, fence: &ID3D12Fence, value: u64) -> Vec<f32> {
        let staging = self.shared_buffer(D3D12_HEAP_TYPE_READBACK, BYTE_SIZE);

        self.record(|list| unsafe {
            list.CopyBufferRegion(&staging, 0, source, 0, BYTE_SIZE as u64);
        });

        self.wait_for(fence, value);

        let mut contents = vec![0.0f32; PIXELS];
        unsafe {
            let mut mapped: *mut c_void = std::ptr::null_mut();
            staging
                .Map(0, None, Some(&mut mapped))
                .expect("mapping the readback buffer should succeed");
            std::ptr::copy_nonoverlapping(
                mapped.cast::<f32>(),
                contents.as_mut_ptr(),
                contents.len(),
            );
            staging.Unmap(0, None);
        }

        contents
    }

    fn record(&self, commands: impl FnOnce(&ID3D12GraphicsCommandList)) {
        unsafe {
            self.allocator
                .Reset()
                .expect("resetting the command allocator should succeed");
            self.list
                .Reset(&self.allocator, None)
                .expect("resetting the command list should succeed");
        }

        commands(&self.list);

        unsafe {
            self.list
                .Close()
                .expect("closing the command list should succeed");
            self.queue
                .ExecuteCommandLists(&[Some(self.list.cast().unwrap())]);
        }
    }

    /// Signals `fence` with `value` on the queue and blocks until it lands.
    fn wait_for(&self, fence: &ID3D12Fence, value: u64) {
        unsafe {
            self.queue
                .Signal(fence, value)
                .expect("signalling the queue fence should succeed");

            if fence.GetCompletedValue() >= value {
                return;
            }

            let event = CreateEventW(None, false, false, None).expect("creating a wait event");
            fence
                .SetEventOnCompletion(value, event)
                .expect("registering the fence event should succeed");

            let wait = WaitForSingleObject(event, TIMEOUT_MS);
            CloseHandle(event).expect("closing the wait event");

            assert_eq!(
                wait, WAIT_OBJECT_0,
                "timed out waiting for the GPU to reach fence value {value}"
            );
        }
    }
}

// --- Cross-process interop -------------------------------------------------

/// Marks the child process, and carries the object names and adapter it should
/// use.
const CHILD_ENV: &str = "OIDN_RS_D3D12_CHILD";
const LUID_ENV: &str = "OIDN_RS_D3D12_LUID";
const FENCE_ENV: &str = "OIDN_RS_D3D12_FENCE";
const INPUT_ENV: &str = "OIDN_RS_D3D12_INPUT";
const OUTPUT_ENV: &str = "OIDN_RS_D3D12_OUTPUT";

/// The same round trip across two processes: the one holding the Direct3D 12
/// objects never touches Open Image Denoise, and the one that denoises never
/// touches Direct3D 12. The fence and buffers are published under Win32 object
/// names and opened by name in the child, which is how an importing process
/// reaches objects whose handles it was never given.
///
/// The child has to outlive the parent's readback. Tearing down the importing
/// process while the exporting device is still using the shared resources
/// removes that device, so the child waits to be told to exit.
#[test]
fn d3d12_named_handles_cross_process_round_trip() {
    if std::env::var_os(CHILD_ENV).is_some() {
        // This process is the child; it runs child_denoises_named_handles.
        return;
    }

    let Some(d3d12) = D3d12::new() else {
        eprintln!("skipped: no Direct3D 12 device");
        return;
    };

    let luid = d3d12.adapter_luid();
    match oidn::Device::by_luid(&luid) {
        Ok(device)
            if device
                .external_semaphore_types()
                .contains(ExternalSemaphoreTypeFlags::D3D12_FENCE) => {}
        Ok(_) => {
            eprintln!("skipped: device cannot import Direct3D 12 fences");
            return;
        }
        Err((err, msg)) => {
            eprintln!("skipped: no OIDN device on the Direct3D 12 adapter: {err:?}: {msg}");
            return;
        }
    }

    // Object names must be unique, and are only reachable while the handles
    // that registered them are open.
    let prefix = format!(r"Local\oidn-rs-{}", std::process::id());
    let (fence_name, input_name, output_name) = (
        format!("{prefix}-fence"),
        format!("{prefix}-input"),
        format!("{prefix}-output"),
    );

    let (noisy, clean) = test_image();

    let input = d3d12.shared_buffer(D3D12_HEAP_TYPE_DEFAULT, BYTE_SIZE);
    let output = d3d12.shared_buffer(D3D12_HEAP_TYPE_DEFAULT, BYTE_SIZE);
    let fence = d3d12.shared_fence();

    let published = [
        d3d12.publish(&input, &input_name),
        d3d12.publish(&output, &output_name),
        d3d12.publish(&fence, &fence_name),
    ];

    d3d12.upload(&input, &noisy);
    unsafe { d3d12.queue.Signal(&fence, 1) }.expect("signalling the upload should succeed");

    let mut child = std::process::Command::new(
        std::env::current_exe().expect("the test binary should have a path"),
    )
    .args(["--exact", "--nocapture", "child_denoises_named_handles"])
    .env(CHILD_ENV, "1")
    .env(LUID_ENV, hex(&luid))
    .env(FENCE_ENV, &fence_name)
    .env(INPUT_ENV, &input_name)
    .env(OUTPUT_ENV, &output_name)
    .stdin(std::process::Stdio::piped())
    .spawn()
    .expect("the denoising child process should start");

    wait_for_child_to_denoise(&fence, &mut child);

    unsafe { d3d12.queue.Wait(&fence, 2) }.expect("waiting on the denoised fence value");
    let denoised = d3d12.download(&output, &fence, 3);

    // Only now may the child let go of the shared resources.
    drop(child.stdin.take());
    let status = child.wait().expect("waiting for the child process");
    assert!(status.success(), "the denoising child process failed");

    for handle in published {
        unsafe { CloseHandle(handle) }.expect("closing a published handle");
    }

    let before = mean_squared_error(&noisy, &clean);
    let after = mean_squared_error(&denoised, &clean);
    println!("mean squared error: {before:.6} before, {after:.6} after");
    assert!(
        after < before,
        "the child process should have denoised into the shared buffer"
    );
}

/// Blocks until the child signals the fence, failing if it exits first or takes
/// too long.
fn wait_for_child_to_denoise(fence: &ID3D12Fence, child: &mut std::process::Child) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(TIMEOUT_MS as u64);

    while std::time::Instant::now() < deadline {
        // A removed device reports u64::MAX, which would otherwise look like
        // the child having signalled.
        let value = unsafe { fence.GetCompletedValue() };
        assert_ne!(
            value,
            u64::MAX,
            "the Direct3D 12 device was removed while the child was running"
        );
        if value >= 2 {
            return;
        }

        if let Some(status) = child.try_wait().expect("polling the child process") {
            panic!("the child process exited before denoising: {status}");
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    panic!("timed out waiting for the child process to denoise");
}

/// The denoising half of [`d3d12_named_handles_cross_process_round_trip`]. It
/// runs in a child process, never creates a Direct3D 12 device, and is a no-op
/// unless that test started it.
#[test]
fn child_denoises_named_handles() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }

    let device = oidn::Device::by_luid(&unhex(&var(LUID_ENV)))
        .expect("an OIDN device on the parent's adapter");

    let input = import_named_buffer(&device, &var(INPUT_ENV));
    let output = import_named_buffer(&device, &var(OUTPUT_ENV));

    let semaphore = unsafe {
        device.create_shared_semaphore_from_win32_handle(
            ExternalSemaphoreTypeFlags::D3D12_FENCE,
            std::ptr::null_mut(),
            Some(&wide(&var(FENCE_ENV))),
        )
    }
    .expect("importing the fence the parent published by name");

    unsafe { device.wait_semaphores_async(&[&semaphore], Some(&[1]), None) }
        .expect("waiting for the parent's upload");

    let mut filter = oidn::RayTracing::try_new(&device).expect("the RT filter should be created");
    filter.srgb(false).image_dimensions(WIDTH, HEIGHT);
    filter
        .filter_buffer(&input, &output)
        .expect("denoising the shared buffers should succeed");

    unsafe { device.signal_semaphores_async(&[&semaphore], Some(&[2])) }
        .expect("signalling the parent that the output is ready");
    device.sync();

    // Hold the imports open until the parent closes our stdin, so that the
    // shared resources outlive its readback.
    let mut ignored = String::new();
    std::io::stdin()
        .read_line(&mut ignored)
        .expect("waiting for the parent to finish");
}

fn import_named_buffer(device: &oidn::Device, name: &str) -> oidn::Buffer {
    unsafe {
        device.create_shared_buffer_from_win32_handle(
            ExternalMemoryTypeFlags::D3D12_RESOURCE | ExternalMemoryTypeFlags::DEDICATED,
            std::ptr::null_mut(),
            Some(&wide(name)),
            BYTE_SIZE,
        )
    }
    .unwrap_or_else(|(err, msg)| panic!("importing `{name}` by name failed: {err:?}: {msg}"))
}

fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} should be set by the parent process"))
}

/// Win32 object names are NUL-terminated UTF-16.
fn wide(name: &str) -> Vec<u16> {
    name.encode_utf16().chain(std::iter::once(0)).collect()
}

fn hex(bytes: &[u8; 8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> [u8; 8] {
    let mut bytes = [0u8; 8];
    for (byte, pair) in bytes.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).expect("hex digits"), 16)
            .expect("hex digits");
    }
    bytes
}
