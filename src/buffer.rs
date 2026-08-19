use crate::sys::{
    OIDNBuffer, OIDNDevice, oidnGetBufferData, oidnGetBufferSize, oidnGetBufferStorage,
    oidnNewBuffer, oidnNewBufferWithStorage, oidnReadBuffer, oidnReadBufferAsync,
    oidnReleaseBuffer, oidnRetainBuffer, oidnWriteBuffer, oidnWriteBufferAsync,
};
use crate::{Device, Error, Storage};
use std::mem;
use std::os::raw::c_void;
use std::sync::Arc;

pub struct Buffer {
    pub(crate) buf: OIDNBuffer,
    pub(crate) device: OIDNDevice,
    pub(crate) size: usize,
    pub(crate) byte_size: usize,
    pub(crate) device_arc: Arc<u8>,
}

impl Device {
    /// Creates a new buffer holding a copy of `contents`.
    pub fn create_buffer(&self, contents: &[f32]) -> Result<Buffer, (Error, String)> {
        let byte_size = mem::size_of_val(contents);

        self.clear_error();

        let buf = unsafe { oidnNewBuffer(self.0, byte_size) };
        if buf.is_null() {
            return Err(self.take_error(Error::OutOfMemory, "oidnNewBuffer"));
        }

        unsafe {
            oidnWriteBuffer(buf, 0, byte_size, contents.as_ptr() as *const _);
        }

        if let Err(err) = self.get_error() {
            unsafe {
                oidnReleaseBuffer(buf);
            }
            return Err(err);
        }

        Ok(Buffer {
            buf,
            device: self.0,
            size: contents.len(),
            byte_size,
            device_arc: self.1.clone(),
        })
    }

    /// Creates a new uninitialized buffer with the requested storage mode.
    ///
    /// The size is expressed as a number of `f32` values to match the rest of
    /// the safe buffer API.
    pub fn create_buffer_with_storage(
        &self,
        len: usize,
        storage: Storage,
    ) -> Result<Buffer, (Error, String)> {
        let byte_size = len.checked_mul(mem::size_of::<f32>()).ok_or((
            Error::InvalidImageDimensions,
            "buffer size overflow".to_string(),
        ))?;

        self.clear_error();

        let buf =
            unsafe { oidnNewBufferWithStorage(self.0, byte_size, storage.as_raw_oidn_storage()) };
        if buf.is_null() {
            return Err(self.take_error(Error::OutOfMemory, "oidnNewBufferWithStorage"));
        }

        Ok(Buffer {
            buf,
            device: self.0,
            size: len,
            byte_size,
            device_arc: self.1.clone(),
        })
    }

    /// # Safety
    /// Raw buffer must not be invalid (e.g. destroyed, null etc.)
    ///
    /// Raw buffer must have been created by this device
    pub unsafe fn create_buffer_from_raw(&self, buffer: OIDNBuffer) -> Buffer {
        let byte_size = unsafe { oidnGetBufferSize(buffer) };
        let size = byte_size / mem::size_of::<f32>();

        Buffer {
            buf: buffer,
            device: self.0,
            size,
            byte_size,
            device_arc: self.1.clone(),
        }
    }

    pub(crate) fn same_device_as_buf(&self, buf: &Buffer) -> bool {
        Arc::ptr_eq(&self.1, &buf.device_arc)
    }

    /// Starts an asynchronous write to an OIDN buffer.
    ///
    /// The returned guard keeps the buffer and source slice borrowed until the
    /// device has been synchronized. Dropping the guard synchronizes the
    /// device as a convenience, but leaking it prevents synchronization.
    ///
    /// # Safety
    ///
    /// The returned guard must not be leaked with mechanisms such as
    /// [`std::mem::forget`] or [`std::mem::ManuallyDrop`]. It must be waited
    /// or dropped before the source slice or buffer are accessed, mutated, or
    /// released.
    ///
    /// The safe Buffer API treats buffers as `[f32]`.
    /// If the raw buffer size is not a multiple of `size_of::<f32>()`,
    /// trailing bytes are inaccessible through the safe API.
    pub unsafe fn write_buffer_async<'a>(
        &'a self,
        buf: &'a mut Buffer,
        contents: &'a [f32],
    ) -> Result<PendingBufferWrite<'a>, (Error, String)> {
        if !self.same_device_as_buf(buf) {
            return Err((
                Error::InvalidArgument,
                "buffer was not created by this device".to_string(),
            ));
        }

        if buf.size != contents.len() {
            return Err((
                Error::InvalidImageDimensions,
                "buffer and source slice sizes do not match".to_string(),
            ));
        }

        self.clear_error();

        unsafe {
            oidnWriteBufferAsync(
                buf.buf,
                0,
                mem::size_of_val(contents),
                contents.as_ptr() as *const _,
            );
        }

        self.get_error()?;

        Ok(PendingBufferWrite {
            device: self,
            _buffer: buf,
            _contents: contents,
            complete: false,
        })
    }

    /// Starts an asynchronous read from an OIDN buffer.
    ///
    /// The returned guard keeps the buffer and destination slice borrowed until
    /// the device has been synchronized. Dropping the guard synchronizes the
    /// device as a convenience, but leaking it prevents synchronization.
    ///
    /// # Safety
    ///
    /// The returned guard must not be leaked with mechanisms such as
    /// [`std::mem::forget`] or [`std::mem::ManuallyDrop`]. It must be waited
    /// or dropped before the destination slice or buffer are accessed, mutated,
    /// or released.
    ///
    /// The safe Buffer API treats buffers as `[f32]`.
    /// If the raw buffer size is not a multiple of `size_of::<f32>()`,
    /// trailing bytes are inaccessible through the safe API.
    pub unsafe fn read_buffer_async<'a>(
        &'a self,
        buf: &'a mut Buffer,
        contents: &'a mut [f32],
    ) -> Result<PendingBufferRead<'a>, (Error, String)> {
        if !self.same_device_as_buf(buf) {
            return Err((
                Error::InvalidArgument,
                "buffer was not created by this device".to_string(),
            ));
        }

        if buf.size != contents.len() {
            return Err((
                Error::InvalidImageDimensions,
                "buffer and destination slice sizes do not match".to_string(),
            ));
        }

        self.clear_error();

        unsafe {
            oidnReadBufferAsync(
                buf.buf,
                0,
                mem::size_of_val(contents),
                contents.as_mut_ptr() as *mut _,
            );
        }

        self.get_error()?;

        Ok(PendingBufferRead {
            device: self,
            _buffer: buf,
            _contents: contents,
            complete: false,
        })
    }
}

impl Buffer {
    /// Writes to the buffer, returns [None] if the sizes mismatch
    pub fn write(&self, contents: &[f32]) -> Result<(), (Error, String)> {
        if self.size != contents.len() {
            return Err((
                Error::InvalidImageDimensions,
                "buffer and source slice sizes do not match".to_string(),
            ));
        }

        let byte_size = mem::size_of_val(contents);

        Device::clear_error_on_raw_device(self.device);

        unsafe {
            oidnWriteBuffer(self.buf, 0, byte_size, contents.as_ptr() as *const _);
        }

        Device::error_from_raw_device(self.device)
    }

    /// Reads from the buffer to the array, returns [None] if the sizes mismatch
    pub fn read_to_slice(&self, contents: &mut [f32]) -> Result<(), (Error, String)> {
        if self.size != contents.len() {
            return Err((
                Error::InvalidImageDimensions,
                "buffer and destination slice sizes do not match".to_string(),
            ));
        }

        let byte_size = mem::size_of_val(contents);

        Device::clear_error_on_raw_device(self.device);

        unsafe {
            oidnReadBuffer(self.buf, 0, byte_size, contents.as_mut_ptr() as *mut _);
        }

        Device::error_from_raw_device(self.device)
    }

    /// Reads from the buffer
    pub fn read(&self) -> Result<Vec<f32>, (Error, String)> {
        let mut contents = vec![0.0; self.size];

        Device::clear_error_on_raw_device(self.device);

        unsafe {
            oidnReadBuffer(
                self.buf,
                0,
                self.size * mem::size_of::<f32>(),
                contents.as_mut_ptr() as *mut _,
            );
        }

        Device::error_from_raw_device(self.device)?;

        Ok(contents)
    }

    /// # Safety
    /// Raw buffer must not be made invalid (e.g. by destroying it)
    pub unsafe fn raw(&self) -> OIDNBuffer {
        self.buf
    }

    /// Returns the size of the buffer in bytes.
    pub fn byte_size(&self) -> usize {
        self.byte_size
    }

    /// Returns the storage mode used by the buffer.
    pub fn storage(&self) -> Storage {
        let storage = unsafe { oidnGetBufferStorage(self.buf) };
        Storage::try_from(storage).unwrap_or(Storage::Undefined)
    }

    /// Returns the raw data pointer for host-accessible buffers.
    ///
    /// This pointer may be null, and dereferencing it is unsafe. OIDN only
    /// permits host access for host and managed storage.
    pub fn data_ptr(&self) -> *mut c_void {
        unsafe { oidnGetBufferData(self.buf) }
    }

    /// Returns the size of the buffer as a number of `f32` values.
    pub fn size(&self) -> usize {
        self.size
    }
}

impl Clone for Buffer {
    fn clone(&self) -> Self {
        unsafe {
            oidnRetainBuffer(self.buf);
        }

        Self {
            buf: self.buf,
            device: self.device,
            size: self.size,
            byte_size: self.byte_size,
            device_arc: self.device_arc.clone(),
        }
    }
}

impl From<&Buffer> for Buffer {
    fn from(buffer: &Buffer) -> Self {
        buffer.clone()
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe { oidnReleaseBuffer(self.buf) }
    }
}

/// Completion guard for an asynchronous OIDN buffer write.
///
/// Calling [`PendingBufferWrite::wait`] or dropping this guard blocks until
/// [`Device::sync`] completes.
#[must_use = "dropping the guard blocks to synchronize; call wait() explicitly when possible"]
pub struct PendingBufferWrite<'a> {
    device: &'a Device,
    _buffer: &'a mut Buffer,
    _contents: &'a [f32],
    complete: bool,
}

impl PendingBufferWrite<'_> {
    /// Blocks until the asynchronous write has completed.
    pub fn wait(mut self) {
        self.finish();
    }

    fn finish(&mut self) {
        if !self.complete {
            self.device.sync();
            self.complete = true;
        }
    }
}

impl Drop for PendingBufferWrite<'_> {
    fn drop(&mut self) {
        self.finish();
    }
}

/// Completion guard for an asynchronous OIDN buffer read.
///
/// Calling [`PendingBufferRead::wait`] or dropping this guard blocks until
/// [`Device::sync`] completes.
#[must_use = "dropping the guard blocks to synchronize; call wait() explicitly when possible"]
pub struct PendingBufferRead<'a> {
    device: &'a Device,
    _buffer: &'a mut Buffer,
    _contents: &'a mut [f32],
    complete: bool,
}

impl PendingBufferRead<'_> {
    /// Blocks until the asynchronous read has completed.
    pub fn wait(mut self) {
        self.finish();
    }

    fn finish(&mut self) {
        if !self.complete {
            self.device.sync();
            self.complete = true;
        }
    }
}

impl Drop for PendingBufferRead<'_> {
    fn drop(&mut self) {
        self.finish();
    }
}

unsafe impl Send for Buffer {}
