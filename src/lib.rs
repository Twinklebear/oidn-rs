//! Rust bindings to Intel's
//! [Open Image Denoise](https://github.com/OpenImageDenoise/oidn).
//!
//! Open Image Denoise documentation can be found
//! [here](https://openimagedenoise.github.io/documentation.html).
//!
//! ## Example
//!
//! The crate provides a lightweight wrapper over the Open Image Denoise
//! library, along with raw C bindings exposed under [`oidn::sys`](sys). Below
//! is an example of using the the [`RayTracing`] filter to denoise an image.
//!
//! ```ignore
//! // Load scene, render image, etc.
//!
//! let input_img: Vec<f32> = // A float3 RGB image produced by your renderer.
//! let mut filter_output = vec![0.0f32; input_img.len()];
//!
//! let device = oidn::Device::new().expect("failed to create an OIDN device");
//! oidn::RayTracing::try_new(&device)
//!     .expect("Failed to create the filter")
//!     // Optionally add float3 normal and albedo buffers as well.
//!     .srgb(true)
//!     .image_dimensions(input.width() as usize, input.height() as usize)
//!     .filter(&input_img[..], &mut filter_output[..])
//!     .expect("Filter config error!");
//!
//! // Save out or display filter_output image.
//! ```

use num_enum::TryFromPrimitive;
use std::fmt;

pub mod buffer;
pub mod device;
pub mod filter;
pub mod semaphore;
#[allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]
pub mod sys;
#[cfg(test)]
mod tests;

#[doc(inline)]
pub use buffer::{Buffer, ExternalMemoryTypeFlags, PendingBufferRead, PendingBufferWrite};
#[doc(inline)]
pub use device::Device;
#[doc(inline)]
pub use filter::{PendingFilter, RayTracing};
#[doc(inline)]
pub use semaphore::{ExternalSemaphoreTypeFlags, Semaphore};

/// The kind of failure an [`Error`] reports.
#[repr(i32)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, TryFromPrimitive)]
pub enum ErrorKind {
    None = sys::OIDNError_OIDN_ERROR_NONE,
    Unknown = sys::OIDNError_OIDN_ERROR_UNKNOWN,
    InvalidArgument = sys::OIDNError_OIDN_ERROR_INVALID_ARGUMENT,
    InvalidOperation = sys::OIDNError_OIDN_ERROR_INVALID_OPERATION,
    OutOfMemory = sys::OIDNError_OIDN_ERROR_OUT_OF_MEMORY,
    UnsupportedHardware = sys::OIDNError_OIDN_ERROR_UNSUPPORTED_HARDWARE,
    Canceled = sys::OIDNError_OIDN_ERROR_CANCELLED,
    InvalidImageDimensions,
}

impl ErrorKind {
    /// A short description of the kind of failure.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorKind::None => "no error",
            ErrorKind::Unknown => "unknown error",
            ErrorKind::InvalidArgument => "invalid argument",
            ErrorKind::InvalidOperation => "invalid operation",
            ErrorKind::OutOfMemory => "out of memory",
            ErrorKind::UnsupportedHardware => "unsupported hardware",
            ErrorKind::Canceled => "canceled",
            ErrorKind::InvalidImageDimensions => "invalid image dimensions",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A failure reported by Open Image Denoise, or by this crate's own argument
/// checks.
///
/// Open Image Denoise usually attaches a message describing what went wrong;
/// [`Error::message`] returns it, and it is empty when there was none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
    message: String,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// The kind of failure, for matching on.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// The message Open Image Denoise reported, which may be empty.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self::new(kind, String::new())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.message.is_empty() {
            write!(formatter, "{}", self.kind)
        } else {
            write!(formatter, "{}: {}", self.kind, self.message)
        }
    }
}

impl std::error::Error for Error {}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, TryFromPrimitive, Default)]
pub enum Quality {
    #[default]
    Default = sys::OIDNQuality_OIDN_QUALITY_DEFAULT,
    Balanced = sys::OIDNQuality_OIDN_QUALITY_BALANCED,
    High = sys::OIDNQuality_OIDN_QUALITY_HIGH,
    Fast = sys::OIDNQuality_OIDN_QUALITY_FAST,
}

impl Quality {
    pub fn as_raw_oidn_quality(&self) -> sys::OIDNQuality {
        match self {
            Quality::Default => sys::OIDNQuality_OIDN_QUALITY_DEFAULT,
            Quality::Balanced => sys::OIDNQuality_OIDN_QUALITY_BALANCED,
            Quality::High => sys::OIDNQuality_OIDN_QUALITY_HIGH,
            Quality::Fast => sys::OIDNQuality_OIDN_QUALITY_FAST,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, TryFromPrimitive, Default)]
pub enum Storage {
    #[default]
    Undefined = sys::OIDNStorage_OIDN_STORAGE_UNDEFINED,
    Host = sys::OIDNStorage_OIDN_STORAGE_HOST,
    Device = sys::OIDNStorage_OIDN_STORAGE_DEVICE,
    Managed = sys::OIDNStorage_OIDN_STORAGE_MANAGED,
}

impl Storage {
    pub fn as_raw_oidn_storage(&self) -> sys::OIDNStorage {
        match self {
            Storage::Undefined => sys::OIDNStorage_OIDN_STORAGE_UNDEFINED,
            Storage::Host => sys::OIDNStorage_OIDN_STORAGE_HOST,
            Storage::Device => sys::OIDNStorage_OIDN_STORAGE_DEVICE,
            Storage::Managed => sys::OIDNStorage_OIDN_STORAGE_MANAGED,
        }
    }
}
