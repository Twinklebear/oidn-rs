use crate::{device::Device, sys::*, Error, Quality};

/// A generic ray tracing denoising filter for denoising
/// images produces with Monte Carlo ray tracing methods
/// such as path tracing.
pub struct RayTracing<'a> {
    handle: OIDNFilter,
    device: &'a Device,
    albedo: Option<(OIDNBuffer, usize)>,
    normal: Option<(OIDNBuffer, usize)>,
    hdr: bool,
    input_scale: f32,
    srgb: bool,
    clean_aux: bool,
    img_dims: (usize, usize, usize),
    filter_quality: OIDNQuality,
}

impl<'a> RayTracing<'a> {
    pub fn new(device: &'a Device) -> RayTracing<'a> {
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
            input_scale: f32::NAN,
            srgb: false,
            clean_aux: false,
            img_dims: (0, 0, 0),
            filter_quality: 0,
        }
    }

    /// Sets the quality of the output, the default is high.
    ///
    /// Balanced lowers the precision, if possible, however
    /// some devices will not support this and so
    /// the result (and performance) will stay the same as high.
    /// Balanced is recommended for realtime usages.
    pub fn filter_quality(&mut self, quality: Quality) -> &mut RayTracing<'a> {
        self.filter_quality = quality.as_raw_oidn_quality();
        self
    }

    /// Set input auxiliary images containing the albedo and normals.
    ///
    /// Albedo must have three channels per pixel with values in [0, 1].
    /// Normal must contain the shading normal as three channels per pixel
    /// *world-space* or *view-space* vectors with arbitrary length, values
    /// in `[-1, 1]`.
    pub fn albedo_normal(&mut self, albedo: &[f32], normal: &[f32]) -> &mut RayTracing<'a> {
        match self.albedo {
            None => {
                let byte_size = albedo.len() * 4;
                let albedo_buffer = unsafe { oidnNewBuffer(self.device.0, byte_size) };
                unsafe { oidnWriteBuffer(albedo_buffer, 0, byte_size, albedo.as_ptr() as *const _) }
                self.albedo = Some((albedo_buffer, albedo.len()));
            }
            Some((buffer, _)) => {
                unsafe { oidnWriteBuffer(buffer, 0, albedo.len() * 4, albedo.as_ptr() as *const _) }
                self.albedo = Some((buffer, albedo.len()));
            }
        }
        match self.normal {
            None => {
                let byte_size = normal.len() * 4;
                let normal_buffer = unsafe { oidnNewBuffer(self.device.0, byte_size) };
                unsafe { oidnWriteBuffer(normal_buffer, 0, byte_size, normal.as_ptr() as *const _) }
                self.normal = Some((normal_buffer, normal.len()));
            }
            Some((buffer, _)) => {
                unsafe { oidnWriteBuffer(buffer, 0, normal.len() * 4, normal.as_ptr() as *const _) }
                self.normal = Some((buffer, normal.len()));
            }
        }
        self
    }

    /// Set an input auxiliary image containing the albedo per pixel (three
    /// channels, values in `[0, 1]`).
    pub fn albedo(&mut self, albedo: &[f32]) -> &mut RayTracing<'a> {
        match self.albedo {
            None => {
                let byte_size = albedo.len() * 4;
                let albedo_buffer = unsafe { oidnNewBuffer(self.device.0, byte_size) };
                unsafe { oidnWriteBuffer(albedo_buffer, 0, byte_size, albedo.as_ptr() as *const _) }
                self.albedo = Some((albedo_buffer, albedo.len()));
            }
            Some((buffer, _)) => {
                unsafe { oidnWriteBuffer(buffer, 0, albedo.len() * 4, albedo.as_ptr() as *const _) }
                self.albedo = Some((buffer, albedo.len()));
            }
        }
        self
    }

    /// Set whether the color is HDR.
    pub fn hdr(&mut self, hdr: bool) -> &mut RayTracing<'a> {
        self.hdr = hdr;
        self
    }

    #[deprecated(since = "1.3.1", note = "Please use RayTracing::input_scale instead")]
    pub fn hdr_scale(&mut self, hdr_scale: f32) -> &mut RayTracing<'a> {
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
    pub fn input_scale(&mut self, input_scale: f32) -> &mut RayTracing<'a> {
        self.input_scale = input_scale;
        self
    }

    /// Set whether the color is encoded with the sRGB (or 2.2 gamma) curve (LDR
    /// only) or is linear.
    ///
    /// The output will be encoded with the same curve.
    pub fn srgb(&mut self, srgb: bool) -> &mut RayTracing<'a> {
        self.srgb = srgb;
        self
    }

    /// Set whether the auxiliary feature (albedo, normal) images are
    /// noise-free.
    ///
    /// Recommended for highest quality but should not be enabled for noisy
    /// auxiliary images to avoid residual noise.
    pub fn clean_aux(&mut self, clean_aux: bool) -> &mut RayTracing<'a> {
        self.clean_aux = clean_aux;
        self
    }

    pub fn image_dimensions(&mut self, width: usize, height: usize) -> &mut RayTracing<'a> {
        let buffer_dims = 3 * width * height;
        match self.albedo {
            None => {}
            Some(buffer) => unsafe {
                if buffer.1 != buffer_dims {
                    oidnReleaseBuffer(buffer.0);
                    self.albedo = None;
                }
            },
        }
        match self.normal {
            None => {}
            Some(buffer) => unsafe {
                if buffer.1 != buffer_dims {
                    oidnReleaseBuffer(buffer.0);
                    self.normal = None;
                }
            },
        }
        self.img_dims = (width, height, buffer_dims);
        self
    }

    pub fn filter(&self, color: &[f32], output: &mut [f32]) -> Result<(), Error> {
        self.execute_filter(Some(color), output)
    }

    pub fn filter_in_place(&self, color: &mut [f32]) -> Result<(), Error> {
        self.execute_filter(None, color)
    }

    fn execute_filter(&self, color: Option<&[f32]>, output: &mut [f32]) -> Result<(), Error> {
        let byte_size = self.img_dims.2 * 4;
        if let Some(alb) = self.albedo {
            if alb.1 != self.img_dims.2 {
                return Err(Error::InvalidImageDimensions);
            }
            unsafe {
                oidnSetFilterImage(
                    self.handle,
                    b"albedo\0" as *const _ as _,
                    alb.0,
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
                if norm.1 != self.img_dims.2 {
                    return Err(Error::InvalidImageDimensions);
                }
                unsafe {
                    oidnSetFilterImage(
                        self.handle,
                        b"normal\0" as *const _ as _,
                        norm.0,
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
                if color.len() != self.img_dims.2 {
                    return Err(Error::InvalidImageDimensions);
                }
                color.as_ptr()
            }
            None => {
                if output.len() != self.img_dims.2 {
                    return Err(Error::InvalidImageDimensions);
                }
                output.as_ptr()
            }
        };
        let color_buffer = unsafe { oidnNewBuffer(self.device.0, byte_size) };
        unsafe {
            oidnWriteBuffer(color_buffer, 0, byte_size, color_ptr as *const _);
            oidnSetFilterImage(
                self.handle,
                b"color\0" as *const _ as _,
                color_buffer,
                OIDNFormat_OIDN_FORMAT_FLOAT3,
                self.img_dims.0 as _,
                self.img_dims.1 as _,
                0,
                0,
                0,
            );
        }

        if output.len() != self.img_dims.2 {
            return Err(Error::InvalidImageDimensions);
        }
        let output_buffer = unsafe { oidnNewBuffer(self.device.0, byte_size) };
        unsafe {
            oidnSetFilterImage(
                self.handle,
                b"output\0" as *const _ as _,
                output_buffer,
                OIDNFormat_OIDN_FORMAT_FLOAT3,
                self.img_dims.0 as _,
                self.img_dims.1 as _,
                0,
                0,
                0,
            );
            oidnSetFilterBool(self.handle, b"hdr\0" as *const _ as _, self.hdr);
            oidnSetFilterFloat(
                self.handle,
                b"inputScale\0" as *const _ as _,
                self.input_scale,
            );
            oidnSetFilterBool(self.handle, b"srgb\0" as *const _ as _, self.srgb);
            oidnSetFilterBool(self.handle, b"clean_aux\0" as *const _ as _, self.clean_aux);

            oidnSetFilterInt(
                self.handle,
                b"quality\0" as *const _ as _,
                self.filter_quality as i32,
            );

            oidnCommitFilter(self.handle);
            oidnExecuteFilter(self.handle);

            oidnReadBuffer(output_buffer, 0, byte_size, output.as_mut_ptr() as *mut _);

            oidnReleaseBuffer(color_buffer);
            oidnReleaseBuffer(output_buffer);
        }
        Ok(())
    }
}

impl<'a> Drop for RayTracing<'a> {
    fn drop(&mut self) {
        unsafe {
            oidnReleaseFilter(self.handle);
            oidnReleaseDevice(self.device.0);
            if let Some(norm) = self.normal {
                oidnReleaseBuffer(norm.0);
            }
            if let Some(alb) = self.albedo {
                oidnReleaseBuffer(alb.0);
            }
        }
    }
}

unsafe impl<'a> Send for RayTracing<'a> {}
