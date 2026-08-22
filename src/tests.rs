use crate::{Buffer, Error, ErrorKind, Quality, Storage};
use std::mem;

fn buffer_or_skip(device: &crate::Device, contents: &[f32]) -> Option<Buffer> {
    match device.create_buffer(contents) {
        Ok(buffer) => Some(buffer),
        // resources failing to be created is not the fault of this library
        Err(_) => {
            eprintln!("Test skipped due to buffer creation failing");
            None
        }
    }
}

fn storage_buffer_or_skip(device: &crate::Device, len: usize, storage: Storage) -> Option<Buffer> {
    match device.create_buffer_with_storage(len, storage) {
        Ok(buffer) => Some(buffer),
        // resources failing to be created is not the fault of this library
        Err(_) => {
            eprintln!("Test skipped due to buffer creation failing");
            None
        }
    }
}

fn assert_device_ok(device: &crate::Device) {
    if let Err(err) = device.get_error() {
        panic!("test failed with {err}")
    }
}

/// Asserts the kind of a failure without pinning the message, which comes from
/// Open Image Denoise for anything it rejects itself.
#[track_caller]
fn assert_error_kind<T>(result: Result<T, Error>, kind: crate::ErrorKind) {
    match result {
        Err(err) => assert_eq!(err.kind(), kind, "unexpected error: {err}"),
        Ok(_) => panic!("expected {kind}, got Ok(..)"),
    }
}

fn assert_send<T: Send>() {}

#[cfg(test)]
#[test]
fn public_types_remain_send() {
    assert_send::<crate::Device>();
    assert_send::<crate::RayTracing<'static>>();
    assert_send::<Buffer>();
    assert_send::<crate::Semaphore>();
}

#[cfg(test)]
#[test]
fn buffer_read_write() {
    let device = crate::Device::cpu();
    let Some(buffer) = buffer_or_skip(&device, &[0.0]) else {
        return;
    };
    buffer.write(&[1.0]).unwrap();
    assert_eq!(buffer.read().unwrap(), vec![1.0]);
    let mut slice = vec![0.0];
    buffer.read_to_slice(&mut slice).unwrap();
    assert_eq!(slice, vec![1.0]);
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn buffer_size_mismatch_paths_return_none() {
    let device = crate::Device::cpu();
    let Some(buffer) = buffer_or_skip(&device, &[0.0]) else {
        return;
    };
    assert!(buffer.write(&[1.0, 2.0]).is_err());

    let mut slice = vec![0.0, 0.0];
    assert!(buffer.read_to_slice(&mut slice).is_err());
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn buffer_import_read_write() {
    let device = crate::Device::cpu();
    let raw_buffer = unsafe { crate::sys::oidnNewBuffer(device.raw(), mem::size_of::<f32>()) };
    if raw_buffer.is_null() {
        eprintln!("Test skipped due to buffer creation failing");
        return;
    }
    let buffer = unsafe { device.create_buffer_from_raw(raw_buffer) };
    buffer.write(&[1.0]).unwrap();
    assert_eq!(buffer.read().unwrap(), vec![1.0]);
    let mut slice = vec![0.0];
    buffer.read_to_slice(&mut slice).unwrap();
    assert_eq!(slice, vec![1.0]);
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn buffer_clone_and_from_keep_raw_buffer_alive() {
    let device = crate::Device::cpu();
    let Some(buffer) = buffer_or_skip(&device, &[1.0]) else {
        return;
    };
    let raw = unsafe { buffer.raw() };
    assert!(!raw.is_null());

    let clone = buffer.clone();
    drop(buffer);
    clone.write(&[2.0]).unwrap();
    assert_eq!(clone.read().unwrap(), vec![2.0]);

    let converted = Buffer::from(&clone);
    drop(clone);
    converted.write(&[3.0]).unwrap();
    assert_eq!(converted.read().unwrap(), vec![3.0]);
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn raw_buffer_byte_size_preserves_non_f32_sizes() {
    let device = crate::Device::cpu();
    let raw_buffer = unsafe { crate::sys::oidnNewBuffer(device.raw(), 1) };
    if raw_buffer.is_null() {
        eprintln!("Test skipped due to buffer creation failing");
        return;
    }
    let buffer = unsafe { device.create_buffer_from_raw(raw_buffer) };
    assert_eq!(buffer.byte_size(), 1);
    assert_eq!(buffer.size(), 0);
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn create_buffer_with_storage_tracks_host_metadata() {
    let device = crate::Device::cpu();
    let Some(buffer) = storage_buffer_or_skip(&device, 2, Storage::Host) else {
        return;
    };

    assert_eq!(buffer.size(), 2);
    assert_eq!(buffer.byte_size(), 2 * mem::size_of::<f32>());
    assert_eq!(buffer.storage(), Storage::Host);
    assert!(!buffer.data_ptr().is_null());

    buffer.write(&[2.0, 3.0]).unwrap();
    assert_eq!(buffer.read().unwrap(), vec![2.0, 3.0]);
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn create_buffer_with_storage_rejects_len_overflow() {
    let device = crate::Device::cpu();
    let overflowing_len = usize::MAX / mem::size_of::<f32>() + 1;
    assert!(
        device
            .create_buffer_with_storage(overflowing_len, Storage::Host)
            .is_err()
    );
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn buffer_async_read_write_waits_for_completion() {
    let device = crate::Device::cpu();
    let Some(mut buffer) = buffer_or_skip(&device, &[0.0, 0.0]) else {
        return;
    };

    let source = [4.0, 5.0];
    unsafe {
        device
            .write_buffer_async(&mut buffer, &source)
            .expect("matching async write should start")
            .wait();
    }
    assert_eq!(buffer.read().unwrap(), source);

    let mut output = [0.0, 0.0];
    unsafe {
        device
            .read_buffer_async(&mut buffer, &mut output)
            .expect("matching async read should start")
            .wait();
    }
    assert_eq!(output, source);
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn buffer_async_rejects_mismatched_lengths_and_devices() {
    let device = crate::Device::cpu();
    let foreign_device = crate::Device::cpu();
    let Some(mut buffer) = buffer_or_skip(&device, &[0.0]) else {
        return;
    };

    assert!(unsafe { device.write_buffer_async(&mut buffer, &[1.0, 2.0]) }.is_err());

    let mut output = [0.0, 0.0];
    assert!(unsafe { device.read_buffer_async(&mut buffer, &mut output) }.is_err());

    assert!(unsafe { foreign_device.write_buffer_async(&mut buffer, &[1.0]) }.is_err());

    let mut read_target = [0.0];
    assert!(unsafe { foreign_device.read_buffer_async(&mut buffer, &mut read_target) }.is_err());
    assert_device_ok(&device);
    assert_device_ok(&foreign_device);
}

#[cfg(test)]
#[test]
fn buffer_async_guards_sync_on_drop() {
    let device = crate::Device::cpu();
    let Some(mut buffer) = buffer_or_skip(&device, &[0.0, 0.0]) else {
        return;
    };

    let source = [7.0, 8.0];
    unsafe {
        let _guard = device
            .write_buffer_async(&mut buffer, &source)
            .expect("matching async write should start");
    }
    assert_eq!(buffer.read().unwrap(), source);

    let mut output = [0.0, 0.0];
    unsafe {
        let _guard = device
            .read_buffer_async(&mut buffer, &mut output)
            .expect("matching async read should start");
    }
    assert_eq!(output, source);
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn filter_buffer_paths_execute_and_wait_for_async_guards() {
    let device = crate::Device::cpu();
    let color_data = [0.2, 0.3, 0.4];
    let Some(color) = buffer_or_skip(&device, &color_data) else {
        return;
    };
    let Some(output) = buffer_or_skip(&device, &[0.0, 0.0, 0.0]) else {
        return;
    };
    let Some(mut async_output) = buffer_or_skip(&device, &[0.0, 0.0, 0.0]) else {
        return;
    };
    let Some(mut in_place) = buffer_or_skip(&device, &color_data) else {
        return;
    };
    let Some(in_place_sync) = buffer_or_skip(&device, &color_data) else {
        return;
    };

    let mut filter = crate::RayTracing::try_new(&device).unwrap();
    filter
        .hdr(false)
        .srgb(true)
        .clean_aux(true)
        .input_scale(1.0)
        .image_dimensions(1, 1);

    filter.filter_buffer(&color, &output).unwrap();
    filter.filter_in_place_buffer(&in_place_sync).unwrap();
    unsafe {
        filter
            .filter_buffer_async(&color, &mut async_output)
            .unwrap()
            .wait();
        filter
            .filter_in_place_buffer_async(&mut in_place)
            .unwrap()
            .wait();
    }
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn filter_async_guard_syncs_on_drop() {
    let device = crate::Device::cpu();
    let color_data = [0.2, 0.3, 0.4];
    let Some(color) = buffer_or_skip(&device, &color_data) else {
        return;
    };
    let Some(mut output) = buffer_or_skip(&device, &[0.0, 0.0, 0.0]) else {
        return;
    };
    let Some(mut in_place) = buffer_or_skip(&device, &color_data) else {
        return;
    };

    let mut filter = crate::RayTracing::try_new(&device).unwrap();
    filter.image_dimensions(1, 1);

    unsafe {
        let _guard = filter
            .filter_buffer_async(&color, &mut output)
            .expect("matching async filter should start");
    }
    unsafe {
        let _guard = filter
            .filter_in_place_buffer_async(&mut in_place)
            .expect("matching in-place async filter should start");
    }
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn slice_filter_paths_execute_and_validate_dimensions() {
    let device = crate::Device::cpu();
    let color = [0.2, 0.3, 0.4];
    let mut output = [0.0, 0.0, 0.0];
    let mut in_place = color;

    let mut filter = crate::RayTracing::try_new(&device).unwrap();
    filter
        .filter_quality(Quality::Default)
        .hdr(false)
        .srgb(true)
        .input_scale(1.0)
        .image_dimensions(1, 1);

    filter.filter(&color, &mut output).unwrap();
    filter.filter_in_place(&mut in_place).unwrap();

    let mut too_short = [0.0, 0.0];
    assert_error_kind(
        filter.filter(&color, &mut too_short),
        ErrorKind::InvalidImageDimensions,
    );
    assert_error_kind(
        filter.filter_in_place(&mut too_short),
        ErrorKind::InvalidImageDimensions,
    );
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn slice_auxiliary_images_are_reused_and_resized() {
    let device = crate::Device::cpu();
    let color = [0.1, 0.2, 0.3];
    let mut output = [0.0, 0.0, 0.0];
    let larger_color = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
    let mut larger_output = [0.0; 6];

    let mut filter = crate::RayTracing::try_new(&device).unwrap();
    filter
        .image_dimensions(1, 1)
        .albedo(&[0.5, 0.5, 0.5])
        .albedo_normal(&[0.6, 0.6, 0.6], &[0.0, 0.0, 1.0]);
    filter.filter(&color, &mut output).unwrap();

    filter.image_dimensions(2, 1);
    filter.filter(&larger_color, &mut larger_output).unwrap();
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
#[allow(deprecated)]
fn slice_auxiliary_setters_cover_initial_reuse_and_resize_paths() {
    let device = crate::Device::cpu();
    let color = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
    let mut output = [0.0; 6];

    let mut filter = crate::RayTracing::try_new(&device).unwrap();
    filter
        .albedo(&[0.5, 0.5, 0.5])
        .albedo(&[0.6, 0.6, 0.6])
        .albedo(&[0.6, 0.6, 0.6, 0.7, 0.7, 0.7])
        .albedo_normal(
            &[0.6, 0.6, 0.6, 0.7, 0.7, 0.7],
            &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        )
        .albedo_normal(&[0.6, 0.6, 0.6], &[0.0, 0.0, 1.0])
        .albedo_normal(
            &[0.7, 0.7, 0.7, 0.8, 0.8, 0.8],
            &[0.1, 0.0, 1.0, 0.1, 0.0, 1.0],
        )
        .hdr_scale(1.0)
        .image_dimensions(2, 1);

    filter.filter(&color, &mut output).unwrap();
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn albedo_only_filter_executes_without_normal() {
    let device = crate::Device::cpu();
    let color = [0.2, 0.3, 0.4];
    let mut output = [0.0, 0.0, 0.0];

    let mut filter = crate::RayTracing::try_new(&device).unwrap();
    filter.image_dimensions(1, 1).albedo(&[0.5, 0.5, 0.5]);
    filter.filter(&color, &mut output).unwrap();
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn filter_buffer_rejects_invalid_dimensions_and_foreign_buffers() {
    let device = crate::Device::cpu();
    let foreign_device = crate::Device::cpu();
    let Some(color) = buffer_or_skip(&device, &[0.0, 0.0, 0.0]) else {
        return;
    };
    let Some(output) = buffer_or_skip(&device, &[0.0, 0.0, 0.0]) else {
        return;
    };
    let Some(foreign_color) = buffer_or_skip(&foreign_device, &[0.0, 0.0, 0.0]) else {
        return;
    };
    let Some(foreign_output) = buffer_or_skip(&foreign_device, &[0.0, 0.0, 0.0]) else {
        return;
    };

    let mut filter = crate::RayTracing::try_new(&device).unwrap();
    filter.image_dimensions(2, 1);
    assert_error_kind(
        filter.filter_buffer(&color, &output),
        ErrorKind::InvalidImageDimensions,
    );

    filter.image_dimensions(1, 1);
    assert_error_kind(
        filter.filter_buffer(&foreign_color, &output),
        ErrorKind::InvalidArgument,
    );
    assert_error_kind(
        filter.filter_buffer(&color, &foreign_output),
        ErrorKind::InvalidArgument,
    );
    assert_error_kind(
        filter.filter_in_place_buffer(&foreign_output),
        ErrorKind::InvalidArgument,
    );
    assert_device_ok(&device);
    assert_device_ok(&foreign_device);
}

#[cfg(test)]
#[test]
fn filter_rejects_auxiliary_buffers_with_invalid_dimensions() {
    let device = crate::Device::cpu();
    let Some(color) = buffer_or_skip(&device, &[0.0, 0.0, 0.0]) else {
        return;
    };
    let Some(output) = buffer_or_skip(&device, &[0.0, 0.0, 0.0]) else {
        return;
    };
    let Some(wide_albedo) = buffer_or_skip(&device, &[0.5; 6]) else {
        return;
    };
    let Some(albedo) = buffer_or_skip(&device, &[0.5, 0.5, 0.5]) else {
        return;
    };
    let Some(wide_normal) = buffer_or_skip(&device, &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0]) else {
        return;
    };

    let mut filter = crate::RayTracing::try_new(&device).unwrap();
    filter.image_dimensions(1, 1);
    assert!(filter.albedo_buffer(&wide_albedo).is_ok());
    assert_error_kind(
        filter.filter_buffer(&color, &output),
        ErrorKind::InvalidImageDimensions,
    );

    let mut filter = crate::RayTracing::try_new(&device).unwrap();
    filter.image_dimensions(1, 1);
    assert!(filter.albedo_normal_buffer(&albedo, &wide_normal).is_ok());
    assert_error_kind(
        filter.filter_buffer(&color, &output),
        ErrorKind::InvalidImageDimensions,
    );

    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn filter_async_rejects_invalid_dimensions_and_foreign_buffers() {
    let device = crate::Device::cpu();
    let foreign_device = crate::Device::cpu();
    let Some(color) = buffer_or_skip(&device, &[0.0, 0.0, 0.0]) else {
        return;
    };
    let Some(too_short) = buffer_or_skip(&device, &[0.0, 0.0]) else {
        return;
    };
    let Some(mut output) = buffer_or_skip(&device, &[0.0, 0.0, 0.0]) else {
        return;
    };
    let Some(mut too_short_output) = buffer_or_skip(&device, &[0.0, 0.0]) else {
        return;
    };
    let Some(foreign_color) = buffer_or_skip(&foreign_device, &[0.0, 0.0, 0.0]) else {
        return;
    };
    let Some(mut foreign_output) = buffer_or_skip(&foreign_device, &[0.0, 0.0, 0.0]) else {
        return;
    };

    let mut filter = crate::RayTracing::try_new(&device).unwrap();
    filter.image_dimensions(1, 1);

    assert_error_kind(
        unsafe { filter.filter_buffer_async(&too_short, &mut output) },
        ErrorKind::InvalidImageDimensions,
    );
    assert_error_kind(
        unsafe { filter.filter_buffer_async(&color, &mut too_short_output) },
        ErrorKind::InvalidImageDimensions,
    );
    assert_error_kind(
        unsafe { filter.filter_buffer_async(&foreign_color, &mut output) },
        ErrorKind::InvalidArgument,
    );
    assert_error_kind(
        unsafe { filter.filter_buffer_async(&color, &mut foreign_output) },
        ErrorKind::InvalidArgument,
    );
    assert_error_kind(
        unsafe { filter.filter_in_place_buffer_async(&mut too_short_output) },
        ErrorKind::InvalidImageDimensions,
    );
    assert_error_kind(
        unsafe { filter.filter_in_place_buffer_async(&mut foreign_output) },
        ErrorKind::InvalidArgument,
    );
    assert_device_ok(&device);
    assert_device_ok(&foreign_device);
}

#[cfg(test)]
#[test]
fn image_dimensions_drops_stale_aux_buffers() {
    let device = crate::Device::cpu();
    let Some(albedo) = buffer_or_skip(&device, &[0.5, 0.5, 0.5]) else {
        return;
    };
    let Some(color) = buffer_or_skip(&device, &[0.0; 6]) else {
        return;
    };
    let Some(output) = buffer_or_skip(&device, &[0.0; 6]) else {
        return;
    };

    let mut filter = crate::RayTracing::try_new(&device).unwrap();
    assert!(filter.image_dimensions(1, 1).albedo_buffer(&albedo).is_ok());

    filter.image_dimensions(2, 1);
    filter.filter_buffer(&color, &output).unwrap();
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn auxiliary_buffer_setters_reject_foreign_devices() {
    let device = crate::Device::cpu();
    let foreign_device = crate::Device::cpu();
    let Some(albedo) = buffer_or_skip(&device, &[0.5, 0.5, 0.5]) else {
        return;
    };
    let Some(normal) = buffer_or_skip(&device, &[0.0, 0.0, 1.0]) else {
        return;
    };
    let Some(foreign_albedo) = buffer_or_skip(&foreign_device, &[0.5, 0.5, 0.5]) else {
        return;
    };
    let Some(foreign_normal) = buffer_or_skip(&foreign_device, &[0.0, 0.0, 1.0]) else {
        return;
    };

    let mut filter = crate::RayTracing::try_new(&device).unwrap();
    assert!(filter.albedo_buffer(&albedo).is_ok());
    assert!(filter.albedo_normal_buffer(&albedo, &normal).is_ok());

    // A foreign buffer is now rejected with the reason, not a bare None.
    assert_error_kind(
        filter.albedo_buffer(&foreign_albedo),
        ErrorKind::InvalidArgument,
    );
    assert_error_kind(
        filter.albedo_normal_buffer(&foreign_albedo, &normal),
        ErrorKind::InvalidArgument,
    );
    assert_error_kind(
        filter.albedo_normal_buffer(&albedo, &foreign_normal),
        ErrorKind::InvalidArgument,
    );
    assert_device_ok(&device);
    assert_device_ok(&foreign_device);
}

#[cfg(test)]
#[test]
fn weights_and_new_enum_variants_are_exposed() {
    let device = crate::Device::cpu();
    let mut filter = crate::RayTracing::try_new(&device).unwrap();
    filter.weights(&[1, 2, 3, 4]).clear_weights();

    assert_eq!(
        Quality::Fast.as_raw_oidn_quality(),
        crate::sys::OIDNQuality_OIDN_QUALITY_FAST
    );
    assert_eq!(
        Quality::try_from(crate::sys::OIDNQuality_OIDN_QUALITY_FAST),
        Ok(Quality::Fast)
    );
    assert_eq!(
        Quality::Default.as_raw_oidn_quality(),
        crate::sys::OIDNQuality_OIDN_QUALITY_DEFAULT
    );
    assert_eq!(
        Quality::Balanced.as_raw_oidn_quality(),
        crate::sys::OIDNQuality_OIDN_QUALITY_BALANCED
    );
    assert_eq!(
        Quality::High.as_raw_oidn_quality(),
        crate::sys::OIDNQuality_OIDN_QUALITY_HIGH
    );
    assert_eq!(
        Quality::try_from(crate::sys::OIDNQuality_OIDN_QUALITY_HIGH),
        Ok(Quality::High)
    );
    assert_eq!(
        Storage::Undefined.as_raw_oidn_storage(),
        crate::sys::OIDNStorage_OIDN_STORAGE_UNDEFINED
    );
    assert_eq!(
        Storage::Host.as_raw_oidn_storage(),
        crate::sys::OIDNStorage_OIDN_STORAGE_HOST
    );
    assert_eq!(
        Storage::Managed.as_raw_oidn_storage(),
        crate::sys::OIDNStorage_OIDN_STORAGE_MANAGED
    );
    assert_eq!(
        Storage::try_from(crate::sys::OIDNStorage_OIDN_STORAGE_DEVICE),
        Ok(Storage::Device)
    );
    assert_eq!(
        Storage::Device.as_raw_oidn_storage(),
        crate::sys::OIDNStorage_OIDN_STORAGE_DEVICE
    );
    assert_eq!(
        ErrorKind::try_from(crate::sys::OIDNError_OIDN_ERROR_CANCELLED),
        Ok(ErrorKind::Canceled)
    );
    assert_eq!(
        ErrorKind::try_from(crate::sys::OIDNError_OIDN_ERROR_INVALID_OPERATION),
        Ok(ErrorKind::InvalidOperation)
    );
    assert_eq!(
        ErrorKind::try_from(crate::sys::OIDNError_OIDN_ERROR_UNSUPPORTED_HARDWARE),
        Ok(ErrorKind::UnsupportedHardware)
    );
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn optional_devices_can_be_queried() {
    // Whether any of these exist depends on the machine; what matters is that
    // an absent one reports why rather than a bare None.
    for device in [
        crate::Device::sycl(),
        crate::Device::cuda(),
        crate::Device::hip(),
        crate::Device::metal(),
    ] {
        if let Err(err) = device {
            assert_ne!(err.kind(), ErrorKind::None);
        }
    }
}

#[cfg(test)]
#[test]
fn default_new_and_from_raw_devices_are_usable() {
    let default_device = crate::Device::default();
    assert_device_ok(&default_device);

    let new_device = crate::Device::new();
    assert_device_ok(&new_device);

    let raw_device =
        unsafe { crate::sys::oidnNewDevice(crate::sys::OIDNDeviceType_OIDN_DEVICE_TYPE_CPU) };
    if raw_device.is_null() {
        eprintln!("Test skipped due to raw device creation failing");
        return;
    }
    unsafe {
        crate::sys::oidnCommitDevice(raw_device);
    }
    let device = unsafe { crate::Device::from_raw(raw_device) };
    let Some(buffer) = buffer_or_skip(&device, &[1.0]) else {
        return;
    };
    assert_eq!(buffer.read().unwrap(), vec![1.0]);
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn stale_device_errors_do_not_fail_unrelated_calls() {
    let device = crate::Device::cpu();

    // Leave an error pending on the device, as a raw `sys` call would. OIDN
    // only records an error when none is stored, so a stale error would both
    // mask real failures and be reported as the failure of the next call.
    unsafe {
        crate::sys::oidnGetDeviceInt(device.raw(), b"nonexistentParameter\0" as *const _ as _);
    }

    let buffer = device
        .create_buffer(&[1.0])
        .expect("a stale device error must not be reported as a buffer failure");
    buffer.write(&[2.0]).unwrap();
    assert_eq!(buffer.read().unwrap(), vec![2.0]);

    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn external_memory_types_can_be_queried() {
    use crate::ExternalMemoryTypeFlags;

    let device = crate::Device::cpu();

    let flags = device.external_memory_types();

    assert_eq!(flags.bits() & !ExternalMemoryTypeFlags::all().bits(), 0);
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn devices_by_unknown_uuid_and_luid_report_an_error() {
    let uuid_error = crate::Device::by_uuid(&[0; 16])
        .err()
        .expect("an all-zero UUID should not match a physical device");
    assert_ne!(uuid_error.kind(), ErrorKind::None);

    let luid_error = crate::Device::by_luid(&[0; 8])
        .err()
        .expect("an all-zero LUID should not match a physical device");
    assert_ne!(luid_error.kind(), ErrorKind::None);
}

#[cfg(test)]
#[test]
fn external_semaphore_types_can_be_queried() {
    use crate::ExternalSemaphoreTypeFlags;

    let device = crate::Device::cpu();

    let flags = device.external_semaphore_types();

    assert_eq!(flags.bits() & !ExternalSemaphoreTypeFlags::all().bits(), 0);
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn semaphore_signal_and_wait_reject_empty_lists() {
    let device = crate::Device::cpu();

    let empty = Error::new(
        ErrorKind::InvalidArgument,
        "semaphore list must not be empty",
    );

    assert_eq!(
        unsafe { device.signal_semaphores_async(&[], None) },
        Err(empty.clone())
    );
    assert_eq!(
        unsafe { device.wait_semaphores_async(&[], None, None) },
        Err(empty)
    );

    assert_device_ok(&device);
}

#[cfg(all(test, unix))]
#[test]
fn invalid_external_semaphore_fd_returns_error() {
    let device = crate::Device::cpu();

    let result = unsafe {
        device.create_shared_semaphore_from_raw_fd(crate::ExternalSemaphoreTypeFlags::OPAQUE_FD, -1)
    };

    assert!(result.is_err());
    assert_device_ok(&device);
}

#[cfg(all(test, windows))]
#[test]
fn invalid_external_semaphore_win32_handle_returns_error() {
    let device = crate::Device::cpu();

    let result = unsafe {
        device.create_shared_semaphore_from_raw_handle(
            crate::ExternalSemaphoreTypeFlags::OPAQUE_WIN32,
            std::ptr::null_mut(),
            None,
        )
    };

    assert!(result.is_err());
    assert_device_ok(&device);
}

#[cfg(all(test, windows))]
#[test]
fn external_semaphore_name_must_be_nul_terminated() {
    let device = crate::Device::cpu();

    let name: Vec<u16> = "oidn-rs".encode_utf16().collect();
    let result = unsafe {
        device.create_shared_semaphore_from_raw_handle(
            crate::ExternalSemaphoreTypeFlags::OPAQUE_WIN32,
            std::ptr::null_mut(),
            Some(&name),
        )
    };

    assert_eq!(
        result.err().map(|err| err.kind()),
        Some(ErrorKind::InvalidArgument)
    );
    assert_device_ok(&device);
}

#[cfg(test)]
#[test]
fn errors_carry_the_device_message() {
    let error = Error::new(
        ErrorKind::InvalidArgument,
        "semaphore list must not be empty",
    );
    assert_eq!(error.kind(), ErrorKind::InvalidArgument);
    assert_eq!(error.message(), "semaphore list must not be empty");
    assert_eq!(
        error.to_string(),
        "invalid argument: semaphore list must not be empty"
    );

    // Not every failure has a message to go with it.
    let bare = Error::from(ErrorKind::OutOfMemory);
    assert!(bare.message().is_empty());
    assert_eq!(bare.to_string(), "out of memory");

    // Usable through the std error trait, so `?` into Box<dyn Error> works.
    let boxed: Box<dyn std::error::Error> = Box::new(error);
    assert_eq!(
        boxed.to_string(),
        "invalid argument: semaphore list must not be empty"
    );
}

#[cfg(test)]
#[test]
fn buffers_outlive_the_device_that_made_them() {
    let device = crate::Device::cpu();
    let Some(buffer) = buffer_or_skip(&device, &[1.0]) else {
        return;
    };

    // The buffer holds its own handle to the device, so dropping this one only
    // gives up our share of it.
    drop(device);

    buffer.write(&[2.0]).unwrap();
    assert_eq!(buffer.read().unwrap(), vec![2.0]);
}
