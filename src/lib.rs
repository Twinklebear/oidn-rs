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
//! let device = oidn::Device::new();
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

#[repr(i32)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, TryFromPrimitive)]
pub enum Error {
    None = sys::OIDNError_OIDN_ERROR_NONE,
    Unknown = sys::OIDNError_OIDN_ERROR_UNKNOWN,
    InvalidArgument = sys::OIDNError_OIDN_ERROR_INVALID_ARGUMENT,
    InvalidOperation = sys::OIDNError_OIDN_ERROR_INVALID_OPERATION,
    OutOfMemory = sys::OIDNError_OIDN_ERROR_OUT_OF_MEMORY,
    UnsupportedFormat = sys::OIDNError_OIDN_ERROR_UNSUPPORTED_HARDWARE,
    Canceled = sys::OIDNError_OIDN_ERROR_CANCELLED,
    InvalidImageDimensions,
}

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
