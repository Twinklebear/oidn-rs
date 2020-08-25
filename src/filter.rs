use std::ffi::CString;
use std::os::raw::c_void;

use device::Device;
use sys::*;
use Format;

#[derive(Debug)]
pub enum FilterError {
    InvalidImageDimensions,
}

/// A generic ray tracing denoising filter for denoising
/// images produces with Monte Carlo ray tracing methods
/// such as path tracing.
pub struct RayTracing<'a> {
    handle: OIDNFilter,
    device: &'a Device,
    hdr: bool,
    srgb: bool,
    img_dims: (usize, usize),
}

impl<'a> RayTracing<'a> {
    pub fn new(device: &'a Device) -> RayTracing<'a> {
        unsafe {
            oidnRetainDevice(device.handle);
        }
        let filter_type = CString::new("RT").unwrap();
        let filter = unsafe { oidnNewFilter(device.handle, filter_type.as_ptr()) };
        RayTracing {
            handle: filter,
            device: device,
            hdr: false,
            srgb: false,
            img_dims: (0, 0),
        }
    }

    pub fn set_hdr(&mut self, hdr: bool) -> &mut RayTracing<'a> {
        self.hdr = hdr;
        self
    }

    pub fn set_srgb(&mut self, srgb: bool) -> &mut RayTracing<'a> {
        self.srgb = srgb;
        self
    }

    pub fn set_img_dims(&mut self, width: usize, height: usize) -> &mut RayTracing<'a> {
        self.img_dims = (width, height);
        self
    }

    pub fn execute(&mut self, color: &[f32], output: &mut [f32]) -> Result<(), FilterError> {
        self.execute_filter(color, None, None, output)
    }

    pub fn execute_with_albedo(
        &mut self,
        color: &[f32],
        albedo: &[f32],
        output: &mut [f32],
    ) -> Result<(), FilterError> {
        self.execute_filter(color, Some(albedo), None, output)
    }

    pub fn execute_with_albedo_normal(
        &mut self,
        color: &[f32],
        albedo: &[f32],
        normal: &[f32],
        output: &mut [f32],
    ) -> Result<(), FilterError> {
        self.execute_filter(color, Some(albedo), Some(normal), output)
    }

    fn execute_filter(
        &mut self,
        color: &[f32],
        albedo: Option<&[f32]>,
        normal: Option<&[f32]>,
        output: &mut [f32],
    ) -> Result<(), FilterError> {
        let buffer_dims = 3 * self.img_dims.0 * self.img_dims.1;
        if let Some(alb) = albedo {
            if alb.len() != buffer_dims {
                return Err(FilterError::InvalidImageDimensions);
            }
            let buf_name = CString::new("albedo").unwrap();
            unsafe {
                oidnSetSharedFilterImage(
                    self.handle,
                    buf_name.as_ptr(),
                    alb.as_ptr() as *mut c_void,
                    Format::FLOAT3,
                    self.img_dims.0,
                    self.img_dims.1,
                    0,
                    0,
                    0,
                );
            }
        }
        if let Some(norm) = normal {
            if norm.len() != buffer_dims {
                return Err(FilterError::InvalidImageDimensions);
            }
            let buf_name = CString::new("normal").unwrap();
            unsafe {
                oidnSetSharedFilterImage(
                    self.handle,
                    buf_name.as_ptr(),
                    norm.as_ptr() as *mut c_void,
                    Format::FLOAT3,
                    self.img_dims.0,
                    self.img_dims.1,
                    0,
                    0,
                    0,
                );
            }
        }

        if color.len() != buffer_dims {
            return Err(FilterError::InvalidImageDimensions);
        }
        let color_buf_name = CString::new("color").unwrap();
        unsafe {
            oidnSetSharedFilterImage(
                self.handle,
                color_buf_name.as_ptr(),
                color.as_ptr() as *mut c_void,
                Format::FLOAT3,
                self.img_dims.0,
                self.img_dims.1,
                0,
                0,
                0,
            );
        }

        if output.len() != buffer_dims {
            return Err(FilterError::InvalidImageDimensions);
        }
        let output_buf_name = CString::new("output").unwrap();
        unsafe {
            oidnSetSharedFilterImage(
                self.handle,
                output_buf_name.as_ptr(),
                output.as_mut_ptr() as *mut c_void,
                Format::FLOAT3,
                self.img_dims.0,
                self.img_dims.1,
                0,
                0,
                0,
            );
        }

        let srgb_name = CString::new("srgb").unwrap();
        let hdr_name = CString::new("hdr").unwrap();
        unsafe {
            oidnSetFilter1b(self.handle, hdr_name.as_ptr(), self.hdr);
            oidnSetFilter1b(self.handle, srgb_name.as_ptr(), self.srgb);

            oidnCommitFilter(self.handle);
            oidnExecuteFilter(self.handle);
        }
        Ok(())
    }
}

impl<'a> Drop for RayTracing<'a> {
    fn drop(&mut self) {
        unsafe {
            oidnReleaseFilter(self.handle);
            oidnReleaseDevice(self.device.handle);
        }
    }
}

unsafe impl<'a> Send for RayTracing<'a> {}
