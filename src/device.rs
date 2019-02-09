use std::ptr;
use std::ffi::CStr;
use std::os::raw::c_char;

use sys::*;
use ::{DeviceType, Error};

pub struct Device {
    pub(crate) handle: OIDNDevice,
}

impl Device {
    /// Create a device using the fastest device available to run denoising
    pub fn new() -> Device {
        let handle = unsafe { oidnNewDevice(DeviceType::DEFAULT) };
        unsafe { oidnCommitDevice(handle); }
        Device { handle: handle }
    }

    /// Create a device to run denoising on the CPU
    pub fn cpu() -> Device {
        let handle = unsafe { oidnNewDevice(DeviceType::CPU) };
        unsafe { oidnCommitDevice(handle); }
        Device { handle: handle }
    }

    pub fn get_error(&mut self) -> Result<(), (Error, String)> {
        let mut err_msg = ptr::null();
        let err = unsafe { oidnGetDeviceError(self.handle, &mut err_msg as *mut *const c_char) };
        if err == Error::NONE {
            Ok(())
        } else {
            let msg = unsafe { CStr::from_ptr(err_msg).to_string_lossy().to_string() };
            Err((err, msg))
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { oidnReleaseDevice(self.handle); }
    }
}

unsafe impl Sync for Device {}

