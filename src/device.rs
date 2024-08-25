use std::{ffi::CStr, os::raw::c_char, ptr};
use std::sync::Arc;
use crate::sys::*;
use crate::Error;

/// An Open Image Denoise device (e.g. a CPU).
///
/// Open Image Denoise supports a device concept, which allows different
/// components of the application to use the API without interfering with each
/// other.
pub struct Device(pub(crate) OIDNDevice, pub(crate) Arc<u8>);

impl Device {
    /// Create a device using the fastest device available to run denoising
    pub fn new() -> Self {
        let handle = unsafe { oidnNewDevice(OIDNDeviceType_OIDN_DEVICE_TYPE_DEFAULT) };
        unsafe {
            oidnCommitDevice(handle);
        }
        Self(handle, Arc::new(0))
    }

    /// Create a device to run denoising on the CPU
    pub fn cpu() -> Self {
        let handle = unsafe { oidnNewDevice(OIDNDeviceType_OIDN_DEVICE_TYPE_CPU) };
        unsafe {
            oidnCommitDevice(handle);
        }
        Self(handle, Arc::new(0))
    }

    pub fn cuda() -> Option<Self> {
        let handle = unsafe { oidnNewDevice(OIDNDeviceType_OIDN_DEVICE_TYPE_CUDA) };
        if handle.is_null() {
            return None;
        }
        unsafe {
            oidnCommitDevice(handle);
        }
        Some(Self(handle, Arc::new(0)))
    }

    pub fn sycl() -> Option<Self> {
        let handle = unsafe { oidnNewDevice(OIDNDeviceType_OIDN_DEVICE_TYPE_SYCL) };
        if handle.is_null() {
            return None;
        }
        unsafe {
            oidnCommitDevice(handle);
        }
        Some(Self(handle, Arc::new(0)))
    }

    pub fn hip() -> Option<Self> {
        let handle = unsafe { oidnNewDevice(OIDNDeviceType_OIDN_DEVICE_TYPE_HIP) };
        if handle.is_null() {
            return None;
        }
        unsafe {
            oidnCommitDevice(handle);
        }
        Some(Self(handle, Arc::new(0)))
    }

    pub fn metal() -> Option<Self> {
        let handle = unsafe { oidnNewDevice(OIDNDeviceType_OIDN_DEVICE_TYPE_METAL) };
        if handle.is_null() {
            return None;
        }
        unsafe {
            oidnCommitDevice(handle);
        }
        Some(Self(handle, Arc::new(0)))
    }

    /// # Safety
    /// Raw device must not be invalid (e.g. destroyed, null, ect.)
    ///
    /// Raw device must be Committed using [oidnCommitDevice]
    pub unsafe fn from_raw(device: OIDNDevice) -> Self {
        Self(device, Arc::new(0))
    }

    /// # Safety
    /// Raw device must not be made invalid (e.g. by destroying it)
    pub unsafe fn raw(&self) -> OIDNDevice {
        self.0
    }

    pub fn get_error(&self) -> Result<(), (Error, String)> {
        let mut err_msg = ptr::null();
        let err = unsafe { oidnGetDeviceError(self.0, &mut err_msg as *mut *const c_char) };
        if OIDNError_OIDN_ERROR_NONE == err {
            Ok(())
        } else {
            let msg = unsafe { CStr::from_ptr(err_msg).to_string_lossy().to_string() };
            Err(((err as u32).try_into().unwrap(), msg))
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            oidnReleaseDevice(self.0);
        }
    }
}

impl Default for Device {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for Device {}
