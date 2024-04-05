use std::{ffi::CStr, os::raw::c_char, ptr};

use crate::sys::*;
use crate::Error;

/// An Open Image Denoise device (e.g. a CPU).
///
/// Open Image Denoise supports a device concept, which allows different
/// components of the application to use the API without interfering with each
/// other.
pub struct Device(pub(crate) OIDNDevice);

impl Device {
    /// Create a device using the fastest device available to run denoising
    pub fn new() -> Self {
        let handle = unsafe { oidnNewDevice(OIDNDeviceType_OIDN_DEVICE_TYPE_DEFAULT) };
        unsafe {
            oidnCommitDevice(handle);
        }
        Self(handle)
    }

    /// Create a device to run denoising on the CPU
    pub fn cpu() -> Self {
        let handle = unsafe { oidnNewDevice(OIDNDeviceType_OIDN_DEVICE_TYPE_CPU) };
        unsafe {
            oidnCommitDevice(handle);
        }
        Self(handle)
    }

    pub fn cuda() -> Option<Self> {
        let handle = unsafe { oidnNewDevice(OIDNDeviceType_OIDN_DEVICE_TYPE_CUDA) };
        if handle.is_null() {
            return None;
        }
        unsafe {
            oidnCommitDevice(handle);
        }
        Some(Self(handle))
    }

    pub fn sycl() -> Option<Self> {
        let handle = unsafe { oidnNewDevice(OIDNDeviceType_OIDN_DEVICE_TYPE_SYCL) };
        if handle.is_null() {
            return None;
        }
        unsafe {
            oidnCommitDevice(handle);
        }
        Some(Self(handle))
    }

    pub fn hip() -> Option<Self> {
        let handle = unsafe { oidnNewDevice(OIDNDeviceType_OIDN_DEVICE_TYPE_HIP) };
        if handle.is_null() {
            return None;
        }
        unsafe {
            oidnCommitDevice(handle);
        }
        Some(Self(handle))
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
