use crate::{buffer::Buffer, device::Device, sys::*, Error, Quality};
use std::mem;

/// A generic ray tracing denoising filter for denoising
/// images produces with Monte Carlo ray tracing methods
/// such as path tracing.
pub struct RayTracing<'a> {
    handle: OIDNFilter,
    device: &'a Device,
    albedo: Option<Buffer>,
    normal: Option<Buffer>,
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
    ///
    /// # Panics
    /// - if resource creation fails
    pub fn albedo_normal(&mut self, albedo: &[f32], normal: &[f32]) -> &mut RayTracing<'a> {
        match self.albedo.as_mut().and_then(|buf| {
            if buf.size == albedo.len() {
                Some(buf)
            } else {
                None
            }
        }) {
            None => {
                self.albedo = Some(self.device.create_buffer(albedo).unwrap());
            }
            Some(buf) => {
                buf.write(albedo)
                    .expect("we check if the size is the same already");
            }
        }
        match self.normal.as_mut().and_then(|buf| {
            if buf.size == normal.len() {
                Some(buf)
            } else {
                None
            }
        }) {
            None => {
                self.albedo = Some(self.device.create_buffer(normal).unwrap());
            }
            Some(buf) => {
                buf.write(normal)
                    .expect("we check if the size is the same already");
            }
        }
        self
    }

    /// Set an input auxiliary image containing the albedo per pixel (three
    /// channels, values in `[0, 1]`).
    ///
    /// # Panics
    /// - if resource creation fails
    pub fn albedo(&mut self, albedo: &[f32]) -> &mut RayTracing<'a> {
        match self.albedo.as_mut().and_then(|buf| {
            if buf.size == albedo.len() {
                Some(buf)
            } else {
                None
            }
        }) {
            None => {
                self.albedo = Some(self.device.create_buffer(albedo).unwrap());
            }
            Some(buf) => {
                buf.write(albedo)
                    .expect("we check if the size is the same already");
            }
        }
        self
    }
    /// Set input auxiliary buffer containing the albedo and normals.
    ///
    /// Albedo buffer must have three channels per pixel with values in [0, 1].
    /// Normal must contain the shading normal as three channels per pixel
    /// *world-space* or *view-space* vectors with arbitrary length, values
    /// in `[-1, 1]`.
    ///
    /// This function is the same as [RayTracing::albedo_normal] but takes buffers instead
    ///
    /// Returns [None] if either buffer was not created by this device
    pub fn albedo_normal_buffer(
        &mut self,
        albedo: Buffer,
        normal: Buffer,
    ) -> Option<&mut RayTracing<'a>> {
        if albedo.id != self.device.0 as isize || normal.id != self.device.0 as isize {
            return None;
        }
        self.albedo = Some(albedo);
        self.normal = Some(normal);
        Some(self)
    }

    /// Set an input auxiliary buffer containing the albedo per pixel (three
    /// channels, values in `[0, 1]`).
    ///
    /// This function is the same as [RayTracing::albedo] but takes buffers instead
    ///
    /// Returns [None] if albedo buffer was not created by this device
    pub fn albedo_buffer(&mut self, albedo: Buffer) -> Option<&mut RayTracing<'a>> {
        if albedo.id != self.device.0 as isize {
            return None;
        }
        self.albedo = Some(albedo);
        Some(self)
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

    /// sets the dimensions of the denoising image, if new width * new height
    /// does not equal old width * old height
    pub fn image_dimensions(&mut self, width: usize, height: usize) -> &mut RayTracing<'a> {
        let buffer_dims = 3 * width * height;
        match &self.albedo {
            None => {}
            Some(buffer) => {
                if buffer.size != buffer_dims {
                    self.albedo = None;
                }
            }
        }
        match &self.normal {
            None => {}
            Some(buffer) => {
                if buffer.size != buffer_dims {
                    self.normal = None;
                }
            }
        }
        self.img_dims = (width, height, buffer_dims);
        self
    }

    pub fn filter(&self, color: &[f32], output: &mut [f32]) -> Result<(), Error> {
        self.execute_filter(Some(color), output)
    }

    pub fn filter_buffer(&self, color: &Buffer, output: &mut Buffer) -> Result<(), Error> {
        self.execute_filter_buffer(Some(color), output)
    }

    pub fn filter_in_place(&self, color: &mut [f32]) -> Result<(), Error> {
        self.execute_filter(None, color)
    }

    pub fn filter_in_place_buffer(&self, color: &mut Buffer) -> Result<(), Error> {
        self.execute_filter_buffer(None, color)
    }

    fn execute_filter(&self, color: Option<&[f32]>, output: &mut [f32]) -> Result<(), Error> {
        let color = match color {
            None => None,
            Some(color) => Some(self.device.create_buffer(color).ok_or(Error::OutOfMemory)?),
        };
        let mut out = self
            .device
            .create_buffer(output)
            .ok_or(Error::OutOfMemory)?;
        self.execute_filter_buffer(color.as_ref(), &mut out)?;
        unsafe {
            oidnReadBuffer(
                out.buf,
                0,
                out.size * mem::size_of::<f32>(),
                output.as_mut_ptr() as *mut _,
            )
        };
        Ok(())
    }

    fn execute_filter_buffer(
        &self,
        color: Option<&Buffer>,
        output: &mut Buffer,
    ) -> Result<(), Error> {
        if let Some(alb) = &self.albedo {
            if alb.size != self.img_dims.2 {
                return Err(Error::InvalidImageDimensions);
            }
            unsafe {
                oidnSetFilterImage(
                    self.handle,
                    b"albedo\0" as *const _ as _,
                    alb.buf,
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
            if let Some(norm) = &self.normal {
                if norm.size != self.img_dims.2 {
                    return Err(Error::InvalidImageDimensions);
                }
                unsafe {
                    oidnSetFilterImage(
                        self.handle,
                        b"normal\0" as *const _ as _,
                        norm.buf,
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
        let color_buffer = match color {
            Some(color) => {
                if color.id != self.device.0 as isize {
                    return Err(Error::InvalidArgument);
                }
                if color.size != self.img_dims.2 {
                    return Err(Error::InvalidImageDimensions);
                }
                color
            }
            None => {
                if output.size != self.img_dims.2 {
                    return Err(Error::InvalidImageDimensions);
                }
                // actually this is a needed borrow, the compiler complains otherwise
                #[allow(clippy::needless_borrow)]
                &output
            }
        };
        unsafe {
            oidnSetFilterImage(
                self.handle,
                b"color\0" as *const _ as _,
                color_buffer.buf,
                OIDNFormat_OIDN_FORMAT_FLOAT3,
                self.img_dims.0 as _,
                self.img_dims.1 as _,
                0,
                0,
                0,
            );
        }
        if output.id != self.device.0 as isize {
            return Err(Error::InvalidArgument);
        }
        if output.size != self.img_dims.2 {
            return Err(Error::InvalidImageDimensions);
        }
        unsafe {
            oidnSetFilterImage(
                self.handle,
                b"output\0" as *const _ as _,
                output.buf,
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
        }
        Ok(())
    }
}

impl<'a> Drop for RayTracing<'a> {
    fn drop(&mut self) {
        unsafe {
            oidnReleaseFilter(self.handle);
            oidnReleaseDevice(self.device.0);
        }
    }
}

unsafe impl<'a> Send for RayTracing<'a> {}
