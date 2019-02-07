#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
pub mod sys;

pub use sys::OIDNFormat as Format;
pub use sys::OIDNAccess as Access;
pub use sys::OIDNDeviceType as DeviceType;
pub use sys::OIDNError as Error;

