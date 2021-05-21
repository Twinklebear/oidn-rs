use crate::{device::Device, sys::*, Error};

/// A generic ray tracing denoising filter for denoising
/// images produces with Monte Carlo ray tracing methods
/// such as path tracing.
pub struct RayTracing<'a, 'b> {
    handle: OIDNFilter,
    device: &'a Device,
    albedo: Option<&'b [f32]>,
    normal: Option<&'b [f32]>,
    hdr: bool,
    input_scale: f32,
    srgb: bool,
    clean_aux: bool,
    img_dims: (usize, usize),
}

impl<'a, 'b> RayTracing<'a, 'b> {
    pub fn new(device: &'a Device) -> RayTracing<'a, 'b> {
        unsafe {
            oidnRetainDevice(device.0);
        }
        let filter = unsafe { oidnNewFilter(device.0, b"RT\0" as *const _ as _) };
        RayTracing {
            handle: filter,
            device,
            albedo: None,
            normal: None,
            hdr: false,
            input_scale: std::f32::NAN,
            srgb: false,
            clean_aux: false,
            img_dims: (0, 0),
        }
    }

    /// Set input auxiliary images containing the albedo and normals.
    ///
    /// Albedo must have three channels per pixel with values in [0, 1].
    /// Normal must contain the shading normal as three channels per pixel
    /// *world-space* or *view-space* vectors with arbitrary length, values
    /// in `[-1, 1]`.
    pub fn albedo_normal(
        &mut self,
        albedo: &'b [f32],
        normal: &'b [f32],
    ) -> &mut RayTracing<'a, 'b> {
        self.albedo = Some(albedo);
        self.normal = Some(normal);
        self
    }

    /// Set an input auxiliary image containing the albedo per pixel (three
    /// channels, values in `[0, 1]`).
    pub fn albedo(&mut self, albedo: &'b [f32]) -> &mut RayTracing<'a, 'b> {
        self.albedo = Some(albedo);
        self
    }

    /// Set whether the color is HDR.
    pub fn hdr(&mut self, hdr: bool) -> &mut RayTracing<'a, 'b> {
        self.hdr = hdr;
        self
    }

    #[deprecated(since = "1.3.1", note = "Please use RayTracing::input_scale instead")]
    pub fn hdr_scale(&mut self, hdr_scale: f32) -> &mut RayTracing<'a, 'b> {
        self.input_scale = hdr_scale;
        self
    }

    /// Sets a scale to apply to input values before filtering, without scaling
    /// the output too.
    ///
    /// This can be used to map color or auxiliary feature values to the
    /// expected range. E.g. for mapping HDR values to physical units (which
    /// affects the quality of the output but not the range of the output
    /// values). If not set, the scale is computed implicitly for HDR images
    /// or set to 1 otherwise
    pub fn input_scale(&mut self, input_scale: f32) -> &mut RayTracing<'a, 'b> {
        self.input_scale = input_scale;
        self
    }

    /// Set whether the color is encoded with the sRGB (or 2.2 gamma) curve (LDR
    /// only) or is linear.
    ///
    /// The output will be encoded with the same curve.
    pub fn srgb(&mut self, srgb: bool) -> &mut RayTracing<'a, 'b> {
        self.srgb = srgb;
        self
    }

    /// Set whether the auxiliary feature (albedo, normal) images are
    /// noise-free.
    ///
    /// Recommended for highest quality but should not be enabled for noisy
    /// auxiliary images to avoid residual noise.
    pub fn clean_aux(&mut self, clean_aux: bool) -> &mut RayTracing<'a, 'b> {
        self.clean_aux = clean_aux;
        self
    }

    pub fn image_dimensions(&mut self, width: usize, height: usize) -> &mut RayTracing<'a, 'b> {
        self.img_dims = (width, height);
        self
    }

    pub fn filter(&self, color: &[f32], output: &mut [f32]) -> Result<(), Error> {
        self.execute_filter(Some(color), output)
    }

    pub fn filter_in_place(&self, color: &mut [f32]) -> Result<(), Error> {
        self.execute_filter(None, color)
    }

    fn execute_filter(&self, color: Option<&[f32]>, output: &mut [f32]) -> Result<(), Error> {
        let buffer_dims = 3 * self.img_dims.0 * self.img_dims.1;

        if let Some(alb) = self.albedo {
            if alb.len() != buffer_dims {
                return Err(Error::InvalidImageDimensions);
            }
            unsafe {
                oidnSetSharedFilterImage(
                    self.handle,
                    b"albedo\0" as *const _ as _,
                    alb.as_ptr() as *mut _,
                    OIDNFormat_OIDN_FORMAT_FLOAT3,
                    self.img_dims.0 as _,
                    self.img_dims.1 as _,
                    0,
                    0,
                    0,
                );
            }

            // No use supplying normal if albedo was
            // not also given.
            if let Some(norm) = self.normal {
                if norm.len() != buffer_dims {
                    return Err(Error::InvalidImageDimensions);
                }
                unsafe {
                    oidnSetSharedFilterImage(
                        self.handle,
                        b"normal\0" as *const _ as _,
                        norm.as_ptr() as *mut _,
                        OIDNFormat_OIDN_FORMAT_FLOAT3,
                        self.img_dims.0 as _,
                        self.img_dims.1 as _,
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
                    return Err(Error::InvalidImageDimensions);
                }
                color.as_ptr()
            }
            None => {
                if output.len() != buffer_dims {
                    return Err(Error::InvalidImageDimensions);
                }
                output.as_ptr()
            }
        };
        unsafe {
            oidnSetSharedFilterImage(
                self.handle,
                b"color\0" as *const _ as _,
                color_ptr as *mut _,
                OIDNFormat_OIDN_FORMAT_FLOAT3,
                self.img_dims.0 as _,
                self.img_dims.1 as _,
                0,
                0,
                0,
            );
        }

        if output.len() != buffer_dims {
            return Err(Error::InvalidImageDimensions);
        }
        unsafe {
            oidnSetSharedFilterImage(
                self.handle,
                b"output\0" as *const _ as _,
                output.as_mut_ptr() as *mut _,
                OIDNFormat_OIDN_FORMAT_FLOAT3,
                self.img_dims.0 as _,
                self.img_dims.1 as _,
                0,
                0,
                0,
            );
        }

        unsafe {
            oidnSetFilter1b(self.handle, b"hdr\0" as *const _ as _, self.hdr);
            oidnSetFilter1f(
                self.handle,
                b"inputScale\0" as *const _ as _,
                self.input_scale,
            );
            oidnSetFilter1b(self.handle, b"srgb\0" as *const _ as _, self.srgb);
            oidnSetFilter1b(self.handle, b"clean_aux\0" as *const _ as _, self.clean_aux);

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
            oidnReleaseDevice(self.device.0);
        }
    }
}

unsafe impl<'a, 'b> Send for RayTracing<'a, 'b> {}
