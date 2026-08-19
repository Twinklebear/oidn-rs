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
use windows::core::Interface;

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
        unsafe {
            self.device.CreateSharedHandle(
                &object.cast::<ID3D12DeviceChild>().unwrap(),
                None,
                GENERIC_ALL,
                None,
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
