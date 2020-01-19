use std::os::raw::c_void;
use std::ffi::CString;

use sys::*;
use ::Format;
use device::Device;

#[derive(Debug)]
pub enum FilterError {
    InvalidImageDimensions
}

/// A generic ray tracing denoising filter for denoising
/// images produces with Monte Carlo ray tracing methods
/// such as path tracing.
pub struct RayTracing<'a, 'b> {
    handle: OIDNFilter,
    device: &'a Device,
    hdr: bool,
    srgb: bool,
    img_dims: (usize, usize),
    albedo: Option<&'b [f32]>,
    normal: Option<&'b [f32]>,
}

impl<'a, 'b> RayTracing<'a, 'b> {
    pub fn new(device: &'a Device) -> RayTracing<'a, 'b> {
        unsafe { oidnRetainDevice(device.handle); }
        let filter_type = CString::new("RT").unwrap();
        let filter = unsafe { oidnNewFilter(device.handle, filter_type.as_ptr()) };
        RayTracing {
            handle: filter,
            device: device,
            hdr: false,
            srgb: false,
            img_dims: (0, 0),
            albedo: None,
            normal: None,
        }
    }

    pub fn set_hdr(&mut self, hdr: bool) -> &mut RayTracing<'a, 'b> {
        self.hdr = hdr;
        self
    }

    pub fn set_srgb(&mut self, srgb: bool) -> &mut RayTracing<'a, 'b> {
        self.srgb = srgb;
        self
    }

    pub fn set_img_dims(&mut self, width: usize, height: usize) -> &mut RayTracing<'a, 'b> {
        self.img_dims = (width, height);
        self
    }

    pub fn set_albedo(&mut self, albedo: &'b [f32]) -> &mut RayTracing<'a, 'b> {
        self.albedo = Some(albedo);
        self
    }

    pub fn set_normal(&mut self, normal: &'b [f32]) -> &mut RayTracing<'a, 'b> {
        self.normal = Some(normal);
        self
    }

    pub fn execute(&mut self, color: &[f32], output: &mut [f32]) -> Result<(), FilterError> {
        let buffer_dims = 3 * self.img_dims.0 * self.img_dims.1;
        if let Some(albedo) = self.albedo {
            if albedo.len() != buffer_dims {
                return Err(FilterError::InvalidImageDimensions);
            }
            let buf_name = CString::new("albedo").unwrap();
            unsafe {
                oidnSetSharedFilterImage(self.handle, buf_name.as_ptr(),
                                         albedo.as_ptr() as *mut c_void,
                                         Format::FLOAT3,
                                         self.img_dims.0, self.img_dims.1,
                                         0, 0, 0);
            }
        }
        if let Some(normal) = self.normal {
            if normal.len() != buffer_dims {
                return Err(FilterError::InvalidImageDimensions);
            }
            let buf_name = CString::new("normal").unwrap();
            unsafe {
                oidnSetSharedFilterImage(self.handle, buf_name.as_ptr(),
                                         normal.as_ptr() as *mut c_void,
                                         Format::FLOAT3,
                                         self.img_dims.0, self.img_dims.1,
                                         0, 0, 0);
            }
        }

        if color.len() != buffer_dims {
            return Err(FilterError::InvalidImageDimensions);
        }
        let color_buf_name = CString::new("color").unwrap();
        unsafe {
            oidnSetSharedFilterImage(self.handle, color_buf_name.as_ptr(),
                                     color.as_ptr() as *mut c_void,
                                     Format::FLOAT3,
                                     self.img_dims.0, self.img_dims.1,
                                     0, 0, 0);
        }

        if output.len() != buffer_dims {
            return Err(FilterError::InvalidImageDimensions);
        }
        let output_buf_name = CString::new("output").unwrap();
        unsafe {
            oidnSetSharedFilterImage(self.handle, output_buf_name.as_ptr(),
                                     output.as_mut_ptr() as *mut c_void,
                                     Format::FLOAT3,
                                     self.img_dims.0, self.img_dims.1,
                                     0, 0, 0);
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

impl<'a, 'b> Drop for RayTracing<'a, 'b> {
    fn drop(&mut self) {
        unsafe {
            oidnReleaseFilter(self.handle);
            oidnReleaseDevice(self.device.handle);
        }
    }
}

unsafe impl<'a, 'b> Send for RayTracing<'a, 'b> {}

