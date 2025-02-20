use std::mem;

#[cfg(test)]
#[test]
fn buffer_read_write() {
    let device = crate::Device::new();
    let mut buffer = match device.create_buffer(&[0.0]) {
        Some(buffer) => buffer,
        // resources failing to be created is not the fault of this library
        None => {
            eprintln!("Test skipped due to buffer creation failing");
            return;
        }
    };
    buffer.write(&[1.0]).unwrap();
    assert_eq!(buffer.read(), vec![1.0]);
    let mut slice = vec![0.0];
    buffer.read_to_slice(&mut slice).unwrap();
    assert_eq!(slice, vec![1.0]);
    if let Err((err, str)) = device.get_error() {
        panic!("test failed with {err:?}: {str}")
    }
}

#[cfg(test)]
#[test]
fn buffer_import_read_write() {
    let device = crate::Device::new();
    let raw_buffer = unsafe { crate::sys::oidnNewBuffer(device.raw(), mem::size_of::<f32>()) };
    if raw_buffer.is_null() {
        eprintln!("Test skipped due to buffer creation failing");
        return;
    }
    let mut buffer = unsafe { device.create_buffer_from_raw(raw_buffer) };
    buffer.write(&[1.0]).unwrap();
    assert_eq!(buffer.read(), vec![1.0]);
    let mut slice = vec![0.0];
    buffer.read_to_slice(&mut slice).unwrap();
    assert_eq!(slice, vec![1.0]);
    if let Err((err, str)) = device.get_error() {
        panic!("test failed with {err:?}: {str}")
    }
}
