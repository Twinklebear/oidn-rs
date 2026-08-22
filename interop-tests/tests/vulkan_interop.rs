//! Interop round trip between Vulkan and Open Image Denoise.
//!
//! The Linux counterpart of `d3d12_interop`, with the same shape: Vulkan
//! uploads a noisy image into a buffer whose memory it exports as a POSIX file
//! descriptor, signals a timeline semaphore it also exported, and Open Image
//! Denoise waits on that semaphore, denoises into a second shared buffer and
//! signals back. The result is copied out through Vulkan alone, so the test
//! only passes if both APIs are looking at the same memory.
//!
//! Importing external memory and semaphores needs a CUDA device on Linux, and
//! a Vulkan driver on the same physical device, so this skips itself unless
//! all of that is present:
//!
//! ```text
//! cargo test -p oidn-interop-tests -- --nocapture
//! ```

#![cfg(unix)]

use std::os::fd::RawFd;

use ash::{Device, Entry, Instance, vk};
use oidn::{ExternalMemoryTypeFlags, ExternalSemaphoreTypeFlags};

const WIDTH: usize = 64;
const HEIGHT: usize = 64;
const PIXELS: usize = WIDTH * HEIGHT * 3;
const BYTE_SIZE: u64 = (PIXELS * size_of::<f32>()) as u64;

/// How long to wait for the round trip before declaring it hung, in
/// nanoseconds, rather than blocking the test run forever.
const TIMEOUT_NS: u64 = 30_000_000_000;

#[test]
fn vulkan_shared_buffer_and_semaphore_round_trip() {
    let Some(vulkan) = Vulkan::new() else {
        return;
    };

    let device = match oidn::Device::by_uuid(&vulkan.device_uuid) {
        Ok(device) => device,
        Err(err) => {
            eprintln!("skipped: no OIDN device on the Vulkan physical device: {err}");
            return;
        }
    };

    let memory_types = device.external_memory_types();
    let semaphore_types = device.external_semaphore_types();
    println!("external memory types: {memory_types:?}");
    println!("external semaphore types: {semaphore_types:?}");

    if !memory_types.contains(ExternalMemoryTypeFlags::OPAQUE_FD)
        || !semaphore_types.contains(ExternalSemaphoreTypeFlags::TIMELINE_SEMAPHORE_FD)
    {
        eprintln!("skipped: device cannot import opaque file descriptors");
        return;
    }

    let (noisy, clean) = test_image();

    // Memory and synchronization shared between the two APIs. Exporting hands
    // the descriptor to us, and importing hands it to Open Image Denoise, so
    // each is exported exactly once.
    let input = vulkan.shared_buffer();
    let output = vulkan.shared_buffer();
    let semaphore = vulkan.timeline_semaphore();

    let shared_input = import_buffer(&device, vulkan.export_memory(input.memory));
    let shared_output = import_buffer(&device, vulkan.export_memory(output.memory));
    let shared_semaphore = unsafe {
        device.create_shared_semaphore_from_raw_fd(
            ExternalSemaphoreTypeFlags::TIMELINE_SEMAPHORE_FD,
            vulkan.export_semaphore(semaphore),
        )
    }
    .expect("importing the exported Vulkan semaphore should succeed");

    // Vulkan: upload the noisy image, then signal that it is ready.
    vulkan.upload(&input, &noisy, semaphore, 1);

    // Open Image Denoise: wait for the upload, denoise, signal completion.
    unsafe { device.wait_semaphores_async(&[&shared_semaphore], Some(&[1]), None) }
        .expect("waiting for the upload semaphore value should be accepted");

    let mut filter = oidn::RayTracing::try_new(&device).expect("the RT filter should be created");
    filter.srgb(false).image_dimensions(WIDTH, HEIGHT);
    filter
        .filter_buffer(&shared_input, &shared_output)
        .expect("denoising the shared buffers should succeed");

    unsafe { device.signal_semaphores_async(&[&shared_semaphore], Some(&[2])) }
        .expect("signalling the denoised semaphore value should be accepted");
    device.sync();

    // Vulkan: wait for the denoised image, then copy it back to the host.
    let denoised = vulkan.download(&output, semaphore, 2, 3);

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

fn import_buffer(device: &oidn::Device, fd: RawFd) -> oidn::Buffer {
    // Importing takes ownership of the descriptor, so there is nothing to
    // close afterwards.
    unsafe {
        device.create_shared_buffer_from_raw_fd(
            ExternalMemoryTypeFlags::OPAQUE_FD,
            fd,
            BYTE_SIZE as usize,
        )
    }
    .expect("importing the exported Vulkan memory should succeed")
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

/// A buffer and the memory backing it, kept together because both are needed
/// to export and to copy.
struct Buffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
}

/// The Vulkan side of the test: an instance, one device with a queue that can
/// copy, and the command pool used for the uploads and downloads.
struct Vulkan {
    _entry: Entry,
    instance: Instance,
    device: Device,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    memory_properties: vk::PhysicalDeviceMemoryProperties,
    device_uuid: [u8; 16],
    external_memory: ash::khr::external_memory_fd::Device,
    external_semaphore: ash::khr::external_semaphore_fd::Device,
}

impl Vulkan {
    /// Returns `None`, after saying why, when the machine cannot run the test.
    fn new() -> Option<Self> {
        let entry = match unsafe { Entry::load() } {
            Ok(entry) => entry,
            Err(error) => {
                eprintln!("skipped: no Vulkan loader: {error}");
                return None;
            }
        };

        let application_info = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_2);
        let instance_info = vk::InstanceCreateInfo::default().application_info(&application_info);
        let instance = match unsafe { entry.create_instance(&instance_info, None) } {
            Ok(instance) => instance,
            Err(error) => {
                eprintln!("skipped: no Vulkan instance: {error}");
                return None;
            }
        };

        let Some((physical_device, queue_family, device_uuid)) =
            Self::pick_physical_device(&instance)
        else {
            eprintln!("skipped: no Vulkan device supporting external memory and semaphores");
            unsafe { instance.destroy_instance(None) };
            return None;
        };

        let queue_infos = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&[1.0])];
        let extensions = [
            ash::khr::external_memory_fd::NAME.as_ptr(),
            ash::khr::external_semaphore_fd::NAME.as_ptr(),
        ];
        let mut features = vk::PhysicalDeviceVulkan12Features::default().timeline_semaphore(true);
        let device_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_infos)
            .enabled_extension_names(&extensions)
            .push_next(&mut features);

        let device = unsafe { instance.create_device(physical_device, &device_info, None) }
            .expect("creating the Vulkan device should succeed");

        let queue = unsafe { device.get_device_queue(queue_family, 0) };
        let command_pool = unsafe {
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(queue_family)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
        }
        .expect("creating the command pool should succeed");

        Some(Self {
            memory_properties: unsafe {
                instance.get_physical_device_memory_properties(physical_device)
            },
            external_memory: ash::khr::external_memory_fd::Device::new(&instance, &device),
            external_semaphore: ash::khr::external_semaphore_fd::Device::new(&instance, &device),
            device_uuid,
            queue,
            command_pool,
            device,
            instance,
            _entry: entry,
        })
    }

    /// Picks the first device with a queue that can transfer, returning its
    /// UUID so that Open Image Denoise can be put on the same one.
    fn pick_physical_device(instance: &Instance) -> Option<(vk::PhysicalDevice, u32, [u8; 16])> {
        let devices = unsafe { instance.enumerate_physical_devices() }.ok()?;

        devices.into_iter().find_map(|physical_device| {
            let queue_family =
                unsafe { instance.get_physical_device_queue_family_properties(physical_device) }
                    .iter()
                    .position(|family| {
                        family
                            .queue_flags
                            .intersects(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
                    })? as u32;

            let mut id_properties = vk::PhysicalDeviceIDProperties::default();
            let mut properties =
                vk::PhysicalDeviceProperties2::default().push_next(&mut id_properties);
            unsafe { instance.get_physical_device_properties2(physical_device, &mut properties) };

            // Vulkan always reports a device UUID; only the LUID has a
            // validity flag, and Open Image Denoise matches on either.
            Some((physical_device, queue_family, id_properties.device_uuid))
        })
    }

    /// Creates a device-local buffer whose memory can be exported.
    fn shared_buffer(&self) -> Buffer {
        let mut external = vk::ExternalMemoryBufferCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
        let buffer = unsafe {
            self.device.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(BYTE_SIZE)
                    .usage(vk::BufferUsageFlags::TRANSFER_SRC | vk::BufferUsageFlags::TRANSFER_DST)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .push_next(&mut external),
                None,
            )
        }
        .expect("creating the shared buffer should succeed");

        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let mut export = vk::ExportMemoryAllocateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
        let memory = unsafe {
            self.device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(requirements.size)
                    .memory_type_index(self.memory_type(
                        requirements.memory_type_bits,
                        vk::MemoryPropertyFlags::DEVICE_LOCAL,
                    ))
                    .push_next(&mut export),
                None,
            )
        }
        .expect("allocating exportable memory should succeed");

        unsafe { self.device.bind_buffer_memory(buffer, memory, 0) }
            .expect("binding the shared buffer should succeed");

        Buffer { buffer, memory }
    }

    fn timeline_semaphore(&self) -> vk::Semaphore {
        let mut semaphore_type = vk::SemaphoreTypeCreateInfo::default()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(0);
        let mut export = vk::ExportSemaphoreCreateInfo::default()
            .handle_types(vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_FD);

        unsafe {
            self.device.create_semaphore(
                &vk::SemaphoreCreateInfo::default()
                    .push_next(&mut semaphore_type)
                    .push_next(&mut export),
                None,
            )
        }
        .expect("creating the exportable timeline semaphore should succeed")
    }

    fn export_memory(&self, memory: vk::DeviceMemory) -> RawFd {
        unsafe {
            self.external_memory.get_memory_fd(
                &vk::MemoryGetFdInfoKHR::default()
                    .memory(memory)
                    .handle_type(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD),
            )
        }
        .expect("exporting the memory should succeed")
    }

    fn export_semaphore(&self, semaphore: vk::Semaphore) -> RawFd {
        unsafe {
            self.external_semaphore.get_semaphore_fd(
                &vk::SemaphoreGetFdInfoKHR::default()
                    .semaphore(semaphore)
                    .handle_type(vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_FD),
            )
        }
        .expect("exporting the semaphore should succeed")
    }

    /// Copies `contents` into `destination`, signalling `signal` once done.
    fn upload(&self, destination: &Buffer, contents: &[f32], signal: vk::Semaphore, value: u64) {
        let staging = self.host_buffer();

        unsafe {
            let mapped = self
                .device
                .map_memory(staging.memory, 0, BYTE_SIZE, vk::MemoryMapFlags::empty())
                .expect("mapping the staging buffer should succeed");
            std::ptr::copy_nonoverlapping(contents.as_ptr(), mapped.cast::<f32>(), contents.len());
            self.device.unmap_memory(staging.memory);
        }

        self.submit(
            |command_buffer| unsafe {
                self.device.cmd_copy_buffer(
                    command_buffer,
                    staging.buffer,
                    destination.buffer,
                    &[vk::BufferCopy::default().size(BYTE_SIZE)],
                );
            },
            &[],
            &[],
            &[signal],
            &[value],
        );

        self.destroy_buffer(staging);
    }

    /// Waits for `wait_value` on `semaphore`, copies `source` back to the
    /// host, and signals `signal_value` when that copy is done.
    fn download(
        &self,
        source: &Buffer,
        semaphore: vk::Semaphore,
        wait_value: u64,
        signal_value: u64,
    ) -> Vec<f32> {
        let staging = self.host_buffer();

        self.submit(
            |command_buffer| unsafe {
                self.device.cmd_copy_buffer(
                    command_buffer,
                    source.buffer,
                    staging.buffer,
                    &[vk::BufferCopy::default().size(BYTE_SIZE)],
                );
            },
            &[semaphore],
            &[wait_value],
            &[semaphore],
            &[signal_value],
        );

        let wait = vk::SemaphoreWaitInfo::default()
            .semaphores(std::slice::from_ref(&semaphore))
            .values(std::slice::from_ref(&signal_value));
        unsafe { self.device.wait_semaphores(&wait, TIMEOUT_NS) }
            .expect("the queue should reach the download semaphore value");

        let mut contents = vec![0.0f32; PIXELS];
        unsafe {
            let mapped = self
                .device
                .map_memory(staging.memory, 0, BYTE_SIZE, vk::MemoryMapFlags::empty())
                .expect("mapping the readback buffer should succeed");
            std::ptr::copy_nonoverlapping(
                mapped.cast::<f32>(),
                contents.as_mut_ptr(),
                contents.len(),
            );
            self.device.unmap_memory(staging.memory);
        }

        self.destroy_buffer(staging);
        contents
    }

    /// A host-visible buffer, used only to get data in and out; it is never
    /// shared with Open Image Denoise.
    fn host_buffer(&self) -> Buffer {
        let buffer = unsafe {
            self.device.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(BYTE_SIZE)
                    .usage(vk::BufferUsageFlags::TRANSFER_SRC | vk::BufferUsageFlags::TRANSFER_DST)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )
        }
        .expect("creating the staging buffer should succeed");

        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let memory = unsafe {
            self.device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(requirements.size)
                    .memory_type_index(self.memory_type(
                        requirements.memory_type_bits,
                        vk::MemoryPropertyFlags::HOST_VISIBLE
                            | vk::MemoryPropertyFlags::HOST_COHERENT,
                    )),
                None,
            )
        }
        .expect("allocating staging memory should succeed");

        unsafe { self.device.bind_buffer_memory(buffer, memory, 0) }
            .expect("binding the staging buffer should succeed");

        Buffer { buffer, memory }
    }

    fn memory_type(&self, allowed: u32, wanted: vk::MemoryPropertyFlags) -> u32 {
        (0..self.memory_properties.memory_type_count)
            .find(|index| {
                allowed & (1 << index) != 0
                    && self.memory_properties.memory_types[*index as usize]
                        .property_flags
                        .contains(wanted)
            })
            .expect("the device should expose a usable memory type")
    }

    /// Records and submits one command buffer, waiting on and signalling the
    /// given timeline values.
    fn submit(
        &self,
        commands: impl FnOnce(vk::CommandBuffer),
        wait: &[vk::Semaphore],
        wait_values: &[u64],
        signal: &[vk::Semaphore],
        signal_values: &[u64],
    ) {
        let command_buffers = unsafe {
            self.device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(self.command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
        }
        .expect("allocating a command buffer should succeed");

        unsafe {
            self.device
                .begin_command_buffer(
                    command_buffers[0],
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .expect("beginning the command buffer should succeed");
        }

        commands(command_buffers[0]);

        unsafe {
            self.device
                .end_command_buffer(command_buffers[0])
                .expect("ending the command buffer should succeed");
        }

        let wait_stages = vec![vk::PipelineStageFlags::TRANSFER; wait.len()];
        let mut timeline = vk::TimelineSemaphoreSubmitInfo::default()
            .wait_semaphore_values(wait_values)
            .signal_semaphore_values(signal_values);
        let submit = vk::SubmitInfo::default()
            .command_buffers(&command_buffers)
            .wait_semaphores(wait)
            .wait_dst_stage_mask(&wait_stages)
            .signal_semaphores(signal)
            .push_next(&mut timeline);

        unsafe {
            self.device
                .queue_submit(self.queue, &[submit], vk::Fence::null())
                .expect("submitting should succeed");
        }
    }

    fn destroy_buffer(&self, buffer: Buffer) {
        unsafe {
            self.device.destroy_buffer(buffer.buffer, None);
            self.device.free_memory(buffer.memory, None);
        }
    }
}

impl Drop for Vulkan {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}
