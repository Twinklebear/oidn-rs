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
/// Therefore, it is recommended to call from the same thread.
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

    /// Creates a device on the physical device with the given UUID, as
    /// reported by e.g. `VkPhysicalDeviceIDProperties::deviceUUID`.
    ///
    /// Sharing memory or semaphores with a graphics API requires the Open
    /// Image Denoise device to run on the same physical device as that API.
    /// Not every physical device supports every identifier, and drivers may
    /// even report inconsistent ones, so check more than one property where
    /// possible.
    pub fn by_uuid(uuid: &[u8; 16]) -> Result<Self, (Error, String)> {
        Self::commit_new_handle(
            unsafe { oidnNewDeviceByUUID(uuid.as_ptr().cast()) },
            "oidnNewDeviceByUUID",
        )
    }

    /// Creates a device on the physical device with the given LUID, as
    /// reported by e.g. `DXGI_ADAPTER_DESC::AdapterLuid` or
    /// `VkPhysicalDeviceIDProperties::deviceLUID`.
    ///
    /// See [`Device::by_uuid`] for the caveats that apply to selecting a
    /// physical device this way.
    pub fn by_luid(luid: &[u8; 8]) -> Result<Self, (Error, String)> {
        Self::commit_new_handle(
            unsafe { oidnNewDeviceByLUID(luid.as_ptr().cast()) },
            "oidnNewDeviceByLUID",
        )
    }

    fn commit_new_handle(handle: OIDNDevice, call: &str) -> Result<Self, (Error, String)> {
        if handle.is_null() {
            // Errors that are not associated with a device, which includes a
            // device failing to be constructed, are reported on the null
            // device.
            return Err(match Self::error_from_raw_device(ptr::null_mut()) {
                Err(err) => err,
                Ok(()) => (
                    Error::Unknown,
                    format!("{call} returned null without setting an error"),
                ),
            });
        }

        unsafe {
            oidnCommitDevice(handle);
        }

        // Dropping the device on the error path releases the handle.
        let device = Self(handle, Arc::new(0));
        device.get_error()?;

        Ok(device)
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

    /// Returns and clears the error state of this device for the calling
    /// thread.
    ///
    /// Open Image Denoise stores one error code per thread per device, and
    /// only records an error if no previous error is stored. A stale error
    /// therefore masks later failures as well as being reported as the failure
    /// of an unrelated call, so the checked wrappers in this crate clear the
    /// error state before any operation whose only failure signal is that
    /// state. If you call into [`sys`](crate::sys) directly, query the error
    /// before handing the device back to the safe API.
    pub fn get_error(&self) -> Result<(), (Error, String)> {
        Self::error_from_raw_device(self.0)
    }

    pub(crate) fn error_from_raw_device(device: OIDNDevice) -> Result<(), (Error, String)> {
        let mut err_msg: *const c_char = ptr::null();
        let err = unsafe { oidnGetDeviceError(device, &mut err_msg as *mut *const c_char) };

        if OIDNError_OIDN_ERROR_NONE == err {
            Ok(())
        } else {
            let msg = if err_msg.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(err_msg).to_string_lossy().to_string() }
            };

            Err((err.try_into().unwrap_or(Error::Unknown), msg))
        }
    }

    /// Returns the pending device error, falling back to `fallback` when a C
    /// call reported failure by returning null without recording one.
    pub(crate) fn take_error(&self, fallback: Error, call: &str) -> (Error, String) {
        match self.get_error() {
            Err(err) => err,
            Ok(()) => (
                fallback,
                format!("{call} returned null without setting a device error"),
            ),
        }
    }

    /// Discards any error stored for the calling thread on this device.
    pub(crate) fn clear_error(&self) {
        Self::clear_error_on_raw_device(self.0);
    }

    pub(crate) fn clear_error_on_raw_device(device: OIDNDevice) {
        unsafe { oidnGetDeviceError(device, ptr::null_mut()) };
    }

    /// Waits until all asynchronous operations on the device have completed.
    pub fn sync(&self) {
        unsafe {
            oidnSyncDevice(self.0);
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

fn get_handle(device_type: OIDNDeviceType) -> *mut OIDNDeviceImpl {
    unsafe { oidnNewDevice(device_type) }
}
