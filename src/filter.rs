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
    hdr_scale: f32,
    srgb: bool,
    img_dims: (usize, usize),
}

impl<'a> RayTracing<'a> {
    pub fn new(device: &'a Device) -> RayTracing<'a> {
        unsafe {
            oidnRetainDevice(device.handle);
        }
        let filter = unsafe { oidnNewFilter(device.handle, &b"RT\0" as *const _ as _) };
        RayTracing {
            handle: filter,
            device: device,
            hdr: false,
            hdr_scale: std::f32::NAN,
            srgb: false,
            img_dims: (0, 0),
        }
    }

    pub fn set_hdr(&mut self, hdr: bool) -> &mut RayTracing<'a> {
        self.hdr = hdr;
        self
    }

    pub fn set_hdr_scale(&mut self, hdr_scale: f32) -> &mut RayTracing<'a> {
        self.hdr_scale = hdr_scale;
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
            unsafe {
                oidnSetSharedFilterImage(
                    self.handle,
                    &b"albedo\0" as *const _ as _,
                    alb.as_ptr() as *mut _,
                    Format::FLOAT3,
                    self.img_dims.0,
                    self.img_dims.1,
                    0,
                    0,
                    0,
                );
            }

            // No use supplying normal if albedo was
            // not also given.
            if let Some(norm) = normal {
                if norm.len() != buffer_dims {
                    return Err(FilterError::InvalidImageDimensions);
                }
                unsafe {
                    oidnSetSharedFilterImage(
                        self.handle,
                        &b"normal\0" as *const _ as _,
                        norm.as_ptr() as *mut _,
                        Format::FLOAT3,
                        self.img_dims.0,
                        self.img_dims.1,
                        0,
                        0,
                        0,
                    );
                }
            }
        }

        if color.len() != buffer_dims {
            return Err(FilterError::InvalidImageDimensions);
        }
        unsafe {
            oidnSetSharedFilterImage(
                self.handle,
                &b"color\0" as *const _ as _,
                color.as_ptr() as *mut _,
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
        unsafe {
            oidnSetSharedFilterImage(
                self.handle,
                &b"output\0" as *const _ as _,
                output.as_mut_ptr() as *mut _,
                Format::FLOAT3,
                self.img_dims.0,
                self.img_dims.1,
                0,
                0,
                0,
            );
        }

        unsafe {
            oidnSetFilter1b(self.handle, &b"hdr\0" as *const _ as _, self.hdr);
            oidnSetFilter1f(self.handle, &b"hdrScale\0" as *const _ as _, self.hdr_scale);
            oidnSetFilter1b(self.handle, &b"srgb\0" as *const _ as _, self.srgb);

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
