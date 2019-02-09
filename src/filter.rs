use std::os::raw::c_void;
use std::ffi::CString;
use std::mem;

use sys::*;
use ::Format;
use device::Device;

/// A generic ray tracing denoising filter for denoising
/// images produces with Monte Carlo ray tracing methods
/// such as path tracing.
pub struct RayTracing<'a> {
    handle: OIDNFilter,
    device: OIDNDevice,
    hdr: bool,
    srgb: bool,
    img_dims: (usize, usize),
    albedo: Option<&'a [f32]>,
    normal: Option<&'a [f32]>,
}

impl<'a> RayTracing<'a> {
    pub fn new(device: &mut Device) -> RayTracing<'a> {
        unsafe { oidnRetainDevice(device.handle); }
        let filter_type = CString::new("RT").unwrap();
        let filter = unsafe { oidnNewFilter(device.handle, filter_type.as_ptr()) };
        RayTracing {
            handle: filter,
            device: device.handle,
            hdr: false,
            srgb: false,
            img_dims: (0, 0),
            albedo: None,
            normal: None,
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

    pub fn set_albedo(&mut self, albedo: &'a [f32]) -> &mut RayTracing<'a> {
        self.albedo = Some(albedo);
        self
    }

    pub fn set_normal(&mut self, normal: &'a [f32]) -> &mut RayTracing<'a> {
        self.normal = Some(normal);
        self
    }

    pub fn execute(&mut self, color: &[f32], output: &mut [f32]) {
        if let Some(albedo) = self.albedo {
            let buf_name = CString::new("albedo").unwrap();
            unsafe {
                oidnSetSharedFilterImage(self.handle, buf_name.as_ptr(),
                                         mem::transmute(albedo.as_ptr()),
                                         Format::FLOAT3,
                                         self.img_dims.0, self.img_dims.1,
                                         0, 0, 0);
            }
        }
        if let Some(normal) = self.normal {
            let buf_name = CString::new("normal").unwrap();
            unsafe {
                oidnSetSharedFilterImage(self.handle, buf_name.as_ptr(),
                                         mem::transmute(normal.as_ptr()),
                                         Format::FLOAT3,
                                         self.img_dims.0, self.img_dims.1,
                                         0, 0, 0);
            }
        }

        let color_buf_name = CString::new("color").unwrap();
        unsafe {
            oidnSetSharedFilterImage(self.handle, color_buf_name.as_ptr(),
                                     mem::transmute(color.as_ptr()),
                                     Format::FLOAT3,
                                     self.img_dims.0, self.img_dims.1,
                                     0, 0, 0);
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
    }
}

impl<'a> Drop for RayTracing<'a> {
    fn drop(&mut self) {
        unsafe {
            oidnReleaseFilter(self.handle);
            oidnReleaseDevice(self.device);
        }
    }
}

unsafe impl<'a> Sync for RayTracing<'a> {}

