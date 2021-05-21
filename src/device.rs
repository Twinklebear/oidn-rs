use std::{convert::TryInto, ffi::CStr, os::raw::c_char, ptr};

use crate::sys::*;
use crate::Error;

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

    pub fn get_error(&self) -> Result<(), (Error, String)> {
        let mut err_msg = ptr::null();
        let err = unsafe { oidnGetDeviceError(self.0, &mut err_msg as *mut *const c_char) };
        if OIDNError_OIDN_ERROR_NONE == err {
            Ok(())
        } else {
            let msg = unsafe { CStr::from_ptr(err_msg).to_string_lossy().to_string() };
            Err((err.try_into().unwrap(), msg))
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

unsafe impl<'a, 'b> Send for Device {}
