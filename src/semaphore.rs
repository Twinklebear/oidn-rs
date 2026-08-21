use crate::sys::{
    OIDNExternalSemaphoreTypeFlags, OIDNSemaphore, oidnGetDeviceInt, oidnReleaseSemaphore,
    oidnRetainSemaphore, oidnSignalSemaphoresAsync, oidnWaitSemaphoresAsync,
};
use crate::{Device, Error, ErrorKind};
use std::ptr;

#[cfg(unix)]
use std::os::fd::RawFd;

#[cfg(windows)]
use std::os::windows::io::RawHandle;

bitflags::bitflags! {
    /// External semaphore handle types supported by Open Image Denoise.
    ///
    /// Query the types a device supports with
    /// [`Device::external_semaphore_types`].
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct ExternalSemaphoreTypeFlags: u32 {
        /// Opaque POSIX file descriptor handle.
        const OPAQUE_FD =
            crate::sys::OIDNExternalSemaphoreTypeFlag_OIDN_EXTERNAL_SEMAPHORE_TYPE_FLAG_OPAQUE_FD as u32;

        /// Opaque NT handle.
        const OPAQUE_WIN32 =
            crate::sys::OIDNExternalSemaphoreTypeFlag_OIDN_EXTERNAL_SEMAPHORE_TYPE_FLAG_OPAQUE_WIN32 as u32;

        /// Opaque global share (KMT) handle.
        const OPAQUE_WIN32_KMT =
            crate::sys::OIDNExternalSemaphoreTypeFlag_OIDN_EXTERNAL_SEMAPHORE_TYPE_FLAG_OPAQUE_WIN32_KMT as u32;

        /// NT handle referencing a Direct3D 11 fence object.
        const D3D11_FENCE =
            crate::sys::OIDNExternalSemaphoreTypeFlag_OIDN_EXTERNAL_SEMAPHORE_TYPE_FLAG_D3D11_FENCE as u32;

        /// NT handle referencing a Direct3D 12 fence object.
        const D3D12_FENCE =
            crate::sys::OIDNExternalSemaphoreTypeFlag_OIDN_EXTERNAL_SEMAPHORE_TYPE_FLAG_D3D12_FENCE as u32;

        /// NT handle referencing a Direct3D 11 keyed mutex object.
        const KEYED_MUTEX =
            crate::sys::OIDNExternalSemaphoreTypeFlag_OIDN_EXTERNAL_SEMAPHORE_TYPE_FLAG_KEYED_MUTEX as u32;

        /// Global share (KMT) handle referencing a Direct3D 11 keyed mutex
        /// object.
        const KEYED_MUTEX_KMT =
            crate::sys::OIDNExternalSemaphoreTypeFlag_OIDN_EXTERNAL_SEMAPHORE_TYPE_FLAG_KEYED_MUTEX_KMT as u32;

        /// POSIX file descriptor referencing a timeline semaphore.
        const TIMELINE_SEMAPHORE_FD =
            crate::sys::OIDNExternalSemaphoreTypeFlag_OIDN_EXTERNAL_SEMAPHORE_TYPE_FLAG_TIMELINE_SEMAPHORE_FD as u32;

        /// NT handle referencing a timeline semaphore.
        const TIMELINE_SEMAPHORE_WIN32 =
            crate::sys::OIDNExternalSemaphoreTypeFlag_OIDN_EXTERNAL_SEMAPHORE_TYPE_FLAG_TIMELINE_SEMAPHORE_WIN32 as u32;
    }
}

/// An external semaphore imported from a graphics API.
///
/// Semaphores synchronize access to memory shared with another API, imported
/// with `Device::create_shared_buffer_from_raw_fd` or
/// `Device::create_shared_buffer_from_raw_handle`. Importing them is
/// supported only by CUDA and HIP devices, and only for the handle types
/// reported by [`Device::external_semaphore_types`].
pub struct Semaphore {
    pub(crate) semaphore: OIDNSemaphore,
    /// The device the semaphore was imported by, kept alive for as long as the
    /// semaphore is.
    pub(crate) device: Device,
    pub(crate) semaphore_type: ExternalSemaphoreTypeFlags,
}

impl Device {
    /// Returns the external semaphore handle types this device can import.
    ///
    /// The result is empty on devices without external semaphore support,
    /// which includes every CPU device.
    pub fn external_semaphore_types(&self) -> ExternalSemaphoreTypeFlags {
        let flags =
            unsafe { oidnGetDeviceInt(self.0, b"externalSemaphoreTypes\0" as *const _ as _) };

        ExternalSemaphoreTypeFlags::from_bits_truncate(flags as u32)
    }

    /// Imports an external semaphore from a POSIX file descriptor.
    ///
    /// Ownership of `fd` is transferred to Open Image Denoise on success; do
    /// not close it yourself.
    ///
    /// # Safety
    ///
    /// `fd` must be a valid handle for an external semaphore of
    /// `semaphore_type`, exported by an API running on the same physical
    /// device as this one.
    #[cfg(unix)]
    pub unsafe fn create_shared_semaphore_from_raw_fd(
        &self,
        semaphore_type: ExternalSemaphoreTypeFlags,
        fd: RawFd,
    ) -> Result<Semaphore, Error> {
        use crate::sys::oidnNewSharedSemaphoreFromFD;

        self.clear_error();

        let semaphore = unsafe {
            oidnNewSharedSemaphoreFromFD(
                self.0,
                semaphore_type.bits() as OIDNExternalSemaphoreTypeFlags,
                fd,
            )
        };

        if semaphore.is_null() {
            return Err(self.take_error(ErrorKind::Unknown, "oidnNewSharedSemaphoreFromFD"));
        }

        Ok(Semaphore {
            semaphore,
            device: self.retained(),
            semaphore_type,
        })
    }

    /// Imports an external semaphore from a Win32 handle.
    ///
    /// Either `handle` or `name` identifies the object: pass the handle for an
    /// unnamed one, or a NUL-terminated UTF-16 `name` with a null handle to
    /// open a named one, which is how a semaphore reaches a process that did
    /// not create it. Unlike a file descriptor, an NT handle is not consumed
    /// by the import, so close it once this returns.
    ///
    /// # Safety
    ///
    /// `handle` must be a valid handle for an external semaphore of
    /// `semaphore_type`, exported by an API running on the same physical
    /// device as this one.
    #[cfg(windows)]
    pub unsafe fn create_shared_semaphore_from_raw_handle(
        &self,
        semaphore_type: ExternalSemaphoreTypeFlags,
        handle: RawHandle,
        name: Option<&[u16]>,
    ) -> Result<Semaphore, Error> {
        use crate::sys::oidnNewSharedSemaphoreFromWin32Handle;

        if let Some(name) = name
            && name.last() != Some(&0)
        {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "semaphore name must be NUL-terminated UTF-16",
            ));
        }

        self.clear_error();

        let semaphore = unsafe {
            oidnNewSharedSemaphoreFromWin32Handle(
                self.0,
                semaphore_type.bits() as OIDNExternalSemaphoreTypeFlags,
                handle as *mut _,
                name.map_or(ptr::null(), |name| name.as_ptr() as *const _),
            )
        };

        if semaphore.is_null() {
            return Err(
                self.take_error(ErrorKind::Unknown, "oidnNewSharedSemaphoreFromWin32Handle")
            );
        }

        Ok(Semaphore {
            semaphore,
            device: self.retained(),
            semaphore_type,
        })
    }

    /// Signals external semaphores asynchronously on this device.
    ///
    /// `values` is only used by semaphore types that carry an explicit value,
    /// such as timeline semaphores, D3D fences (the value to set) and keyed
    /// mutexes (the key); pass `None` for binary semaphores.
    ///
    /// # Safety
    ///
    /// The signal is asynchronous: every semaphore must stay alive, and the
    /// object it was imported from must stay valid, until the operation
    /// completes.
    pub unsafe fn signal_semaphores_async(
        &self,
        semaphores: &[&Semaphore],
        values: Option<&[u64]>,
    ) -> Result<(), Error> {
        let raw_semaphores = self.raw_semaphores(semaphores, values, None)?;

        self.clear_error();

        unsafe {
            oidnSignalSemaphoresAsync(
                self.0,
                raw_semaphores.as_ptr(),
                values.map_or(ptr::null(), |values| values.as_ptr()),
                raw_semaphores.len() as _,
            );
        }

        self.get_error()
    }

    /// Waits for external semaphores asynchronously on this device.
    ///
    /// `values` and `timeouts_ms` are only used by semaphore types that carry
    /// an explicit value or support timeouts; pass `None` otherwise.
    ///
    /// # Safety
    ///
    /// The wait is asynchronous: every semaphore must stay alive, and the
    /// object it was imported from must stay valid, until the operation
    /// completes.
    pub unsafe fn wait_semaphores_async(
        &self,
        semaphores: &[&Semaphore],
        values: Option<&[u64]>,
        timeouts_ms: Option<&[u32]>,
    ) -> Result<(), Error> {
        let raw_semaphores = self.raw_semaphores(semaphores, values, timeouts_ms)?;

        self.clear_error();

        unsafe {
            oidnWaitSemaphoresAsync(
                self.0,
                raw_semaphores.as_ptr(),
                values.map_or(ptr::null(), |values| values.as_ptr()),
                timeouts_ms.map_or(ptr::null(), |timeouts_ms| timeouts_ms.as_ptr()),
                raw_semaphores.len() as _,
            );
        }

        self.get_error()
    }

    fn raw_semaphores(
        &self,
        semaphores: &[&Semaphore],
        values: Option<&[u64]>,
        timeouts_ms: Option<&[u32]>,
    ) -> Result<Vec<OIDNSemaphore>, Error> {
        validate_semaphore_counts(
            semaphores.len(),
            values.map(<[u64]>::len),
            timeouts_ms.map(<[u32]>::len),
        )?;

        for semaphore in semaphores {
            if !self.is_same_device(&semaphore.device) {
                return Err(Error::new(
                    ErrorKind::InvalidArgument,
                    "semaphore was not created by this device",
                ));
            }
        }

        Ok(semaphores
            .iter()
            .map(|semaphore| semaphore.semaphore)
            .collect())
    }
}

/// Checks the list lengths a signal or wait was given. Split out from the
/// semaphores themselves so that it is testable without a device that supports
/// importing them.
fn validate_semaphore_counts(
    semaphores: usize,
    values: Option<usize>,
    timeouts_ms: Option<usize>,
) -> Result<(), Error> {
    if semaphores == 0 {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "semaphore list must not be empty",
        ));
    }

    if values.is_some_and(|values| values != semaphores) {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "semaphore values length does not match semaphore count",
        ));
    }

    if timeouts_ms.is_some_and(|timeouts_ms| timeouts_ms != semaphores) {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "semaphore timeout length does not match semaphore count",
        ));
    }

    Ok(())
}

impl Semaphore {
    /// Returns the raw OIDN semaphore handle.
    ///
    /// # Safety
    ///
    /// The returned handle must not be released, as this [`Semaphore`] still
    /// owns a reference to it.
    pub unsafe fn raw(&self) -> OIDNSemaphore {
        self.semaphore
    }

    /// Returns the external semaphore type this semaphore was imported as.
    pub fn semaphore_type(&self) -> ExternalSemaphoreTypeFlags {
        self.semaphore_type
    }
}

impl Clone for Semaphore {
    fn clone(&self) -> Self {
        unsafe {
            oidnRetainSemaphore(self.semaphore);
        }

        Self {
            semaphore: self.semaphore,
            device: self.device.retained(),
            semaphore_type: self.semaphore_type,
        }
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        unsafe {
            oidnReleaseSemaphore(self.semaphore);
        }
    }
}

unsafe impl Send for Semaphore {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semaphore_counts_must_be_consistent() {
        assert_eq!(validate_semaphore_counts(1, Some(1), Some(1)), Ok(()));
        assert_eq!(validate_semaphore_counts(2, None, None), Ok(()));

        for (semaphores, values, timeouts_ms, message) in [
            (0, None, None, "semaphore list must not be empty"),
            (
                1,
                Some(2),
                None,
                "semaphore values length does not match semaphore count",
            ),
            (
                2,
                Some(2),
                Some(1),
                "semaphore timeout length does not match semaphore count",
            ),
        ] {
            assert_eq!(
                validate_semaphore_counts(semaphores, values, timeouts_ms),
                Err(Error::new(ErrorKind::InvalidArgument, message))
            );
        }
    }
}
