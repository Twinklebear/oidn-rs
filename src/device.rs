use crate::Error;
use crate::sys::*;
use std::sync::Arc;
use std::{ffi::CStr, os::raw::c_char, ptr};

/// An Open Image Denoise device (e.g. a CPU).
///
/// Open Image Denoise supports a device concept, which allows different
/// components of the application to use the API without interfering with each
/// other.
///
/// While all API calls on a device are thread-safe, they may be serialized.
/// Therefor, it is recommended to call from the same thread.
pub struct Device(pub(crate) OIDNDevice, pub(crate) Arc<u8>);

impl Device {
    /// Create a device using the fastest device available to run denoising
    pub fn new() -> Self {
        Self::create(OIDNDeviceType_OIDN_DEVICE_TYPE_DEFAULT)
    }

    fn create(device_type: OIDNDeviceType) -> Self {
        let handle = get_handle(device_type);
        unsafe {
            oidnCommitDevice(handle);
        }
        Self(handle, Arc::new(0))
    }

    fn try_create(device_type: OIDNDeviceType) -> Option<Self> {
        let handle = get_handle(device_type);
        if !handle.is_null() {
            unsafe {
                oidnCommitDevice(handle);
                Some(Self(handle, Arc::new(0)))
            }
        } else {
            None
        }
    }

    pub fn cpu() -> Self {
        Self::create(OIDNDeviceType_OIDN_DEVICE_TYPE_CPU)
    }

    pub fn sycl() -> Option<Self> {
        Self::try_create(OIDNDeviceType_OIDN_DEVICE_TYPE_SYCL)
    }

    pub fn cuda() -> Option<Self> {
        Self::try_create(OIDNDeviceType_OIDN_DEVICE_TYPE_CUDA)
    }

    pub fn hip() -> Option<Self> {
        Self::try_create(OIDNDeviceType_OIDN_DEVICE_TYPE_HIP)
    }

    pub fn metal() -> Option<Self> {
        Self::try_create(OIDNDeviceType_OIDN_DEVICE_TYPE_METAL)
    }

    /// # Safety
    /// Raw device must not be invalid (e.g. destroyed, null, etc.)
    /// Raw device must be committed using [oidnCommitDevice].
    pub unsafe fn from_raw(device: OIDNDevice) -> Self {
        Self(device, Arc::new(0))
    }

    /// # Safety
    /// Raw device must not be made invalid (e.g. by destroying it).
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

fn get_handle(device_type: u32) -> *mut OIDNDeviceImpl {
    unsafe { oidnNewDevice(device_type) }
}
