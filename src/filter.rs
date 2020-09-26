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
pub struct RayTracing<'a, 'b> {
    handle: OIDNFilter,
    device: &'a Device,
    albedo: Option<&'b [f32]>,
    normal: Option<&'b [f32]>,
    hdr: bool,
    hdr_scale: f32,
    srgb: bool,
    img_dims: (usize, usize),
}

impl<'a, 'b> RayTracing<'a, 'b> {
    pub fn new(device: &'a Device) -> RayTracing<'a, 'b> {
        unsafe {
            oidnRetainDevice(device.handle);
        }
        let filter = unsafe { oidnNewFilter(device.handle, b"RT\0" as *const _ as _) };
        RayTracing {
            handle: filter,
            device: device,
            albedo: None,
            normal: None,
            hdr: false,
            hdr_scale: std::f32::NAN,
            srgb: false,
            img_dims: (0, 0),
        }
    }

    pub fn albedo_normal(
        &mut self,
        albedo: &'b [f32],
        normal: &'b [f32],
    ) -> &mut RayTracing<'a, 'b> {
        self.albedo = Some(albedo);
        self.normal = Some(normal);
        self
    }

    pub fn albedo(&mut self, albedo: &'b [f32]) -> &mut RayTracing<'a, 'b> {
        self.albedo = Some(albedo);
        self
    }

    pub fn hdr(&mut self, hdr: bool) -> &mut RayTracing<'a, 'b> {
        self.hdr = hdr;
        self
    }

    pub fn hdr_scale(&mut self, hdr_scale: f32) -> &mut RayTracing<'a, 'b> {
        self.hdr_scale = hdr_scale;
        self
    }

    pub fn srgb(&mut self, srgb: bool) -> &mut RayTracing<'a, 'b> {
        self.srgb = srgb;
        self
    }

    pub fn image_dimensions(&mut self, width: usize, height: usize) -> &mut RayTracing<'a, 'b> {
        self.img_dims = (width, height);
        self
    }

    pub fn filter(&self, color: &[f32], output: &mut [f32]) -> Result<(), FilterError> {
        self.execute_filter(Some(color), output)
    }

    pub fn filter_in_place(&self, color: &mut [f32]) -> Result<(), FilterError> {
        self.execute_filter(None, color)
    }

    fn execute_filter(&self, color: Option<&[f32]>, output: &mut [f32]) -> Result<(), FilterError> {
        let buffer_dims = 3 * self.img_dims.0 * self.img_dims.1;

        if let Some(alb) = self.albedo {
            if alb.len() != buffer_dims {
                return Err(FilterError::InvalidImageDimensions);
            }
            unsafe {
                oidnSetSharedFilterImage(
                    self.handle,
                    b"albedo\0" as *const _ as _,
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
            if let Some(norm) = self.normal {
                if norm.len() != buffer_dims {
                    return Err(FilterError::InvalidImageDimensions);
                }
                unsafe {
                    oidnSetSharedFilterImage(
                        self.handle,
                        b"normal\0" as *const _ as _,
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

        let color_ptr = match color {
            Some(color) => {
                if color.len() != buffer_dims {
                    return Err(FilterError::InvalidImageDimensions);
                }
                color.as_ptr()
            }
            None => {
                if output.len() != buffer_dims {
                    return Err(FilterError::InvalidImageDimensions);
                }
                output.as_ptr()
            }
        };
        unsafe {
            oidnSetSharedFilterImage(
                self.handle,
                b"color\0" as *const _ as _,
                color_ptr as *mut _,
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
                b"output\0" as *const _ as _,
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
            oidnSetFilter1b(self.handle, b"hdr\0" as *const _ as _, self.hdr);
            oidnSetFilter1f(self.handle, b"hdrScale\0" as *const _ as _, self.hdr_scale);
            oidnSetFilter1b(self.handle, b"srgb\0" as *const _ as _, self.srgb);

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
