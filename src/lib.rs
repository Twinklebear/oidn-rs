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
//! oidn::RayTracing::new(&device)
//!     // Optionally add float3 normal and albedo buffers as well.
//!     .srgb(true)
//!     .image_dimensions(input.width() as usize, input.height() as usize);
//!     .filter(&input_img[..], &mut filter_output[..])
//!     .expect("Filter config error!");
//!
//! if let Err(e) = device.get_error() {
//!     println!("Error denosing image: {}", e.1);
//! }
//!
//! // Save out or display filter_output image.
//! ```

use num_enum::TryFromPrimitive;

pub mod device;
pub mod filter;
#[allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]
pub mod sys;
pub mod buffer;

#[doc(inline)]
pub use device::Device;
#[doc(inline)]
pub use filter::RayTracing;

#[repr(u32)]
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

#[repr(u32)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, TryFromPrimitive)]
pub enum Quality {
    Default = sys::OIDNQuality_OIDN_QUALITY_DEFAULT,
    Balanced = sys::OIDNQuality_OIDN_QUALITY_BALANCED,
    High = sys::OIDNQuality_OIDN_QUALITY_HIGH,
}

impl Quality {
    pub fn as_raw_oidn_quality(&self) -> sys::OIDNQuality {
        match self {
            Quality::Default => sys::OIDNQuality_OIDN_QUALITY_DEFAULT,
            Quality::Balanced => sys::OIDNQuality_OIDN_QUALITY_BALANCED,
            Quality::High => sys::OIDNQuality_OIDN_QUALITY_HIGH,
        }
    }
}
