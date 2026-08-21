use crate::{Error, ErrorKind, Quality, buffer::Buffer, device::Device, sys::*};
use std::cell::Cell;

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
    weights: Option<Vec<u8>>,
    // Tracks whether OIDN currently holds a shared-data pointer into `weights`.
    weights_set: Cell<bool>,
    img_dims: (usize, usize, usize),
    filter_quality: OIDNQuality,
}

impl<'a> RayTracing<'a> {
    #[deprecated(since = "2.5.0", note = "use RayTracing::try_new instead")]
    pub fn new(device: &'a Device) -> RayTracing<'a> {
        Self::try_new(device).expect("failed to create OIDN RT filter")
    }

    /// Creates an `RT` filter on `device`.
    pub fn try_new(device: &'a Device) -> Result<RayTracing<'a>, Error> {
        unsafe {
            oidnRetainDevice(device.0);
        }

        device.clear_error();

        let filter = unsafe { oidnNewFilter(device.0, b"RT\0" as *const _ as _) };

        if filter.is_null() {
            unsafe {
                oidnReleaseDevice(device.0);
            }

            return Err(device.take_error(ErrorKind::Unknown, "oidnNewFilter"));
        }

        Ok(RayTracing {
            handle: filter,
            device,
            albedo: None,
            normal: None,
            hdr: false,
            input_scale: f32::NAN,
            srgb: false,
            clean_aux: false,
            weights: None,
            weights_set: Cell::new(false),
            img_dims: (0, 0, 0),
            filter_quality: 0,
        })
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
        match self.albedo.as_mut().filter(|buf| buf.size == albedo.len()) {
            None => {
                self.albedo = Some(self.device.create_buffer(albedo).unwrap());
            }
            Some(buf) => {
                buf.write(albedo)
                    .expect("we check if the size is the same already");
            }
        }
        match self.normal.as_mut().filter(|buf| buf.size == normal.len()) {
            None => {
                self.normal = Some(self.device.create_buffer(normal).unwrap());
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
        match self.albedo.as_mut().filter(|buf| buf.size == albedo.len()) {
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
    /// This function is the same as [RayTracing::albedo_normal] but takes
    /// buffers instead
    ///
    /// Returns [None] if either buffer was not created by this device
    pub fn albedo_normal_buffer(
        &mut self,
        albedo: impl Into<Buffer>,
        normal: impl Into<Buffer>,
    ) -> Option<&mut RayTracing<'a>> {
        let albedo = albedo.into();
        let normal = normal.into();
        if !self.device.same_device_as_buf(&albedo) || !self.device.same_device_as_buf(&normal) {
            return None;
        }
        self.albedo = Some(albedo);
        self.normal = Some(normal);
        Some(self)
    }

    /// Set an input auxiliary buffer containing the albedo per pixel (three
    /// channels, values in `[0, 1]`).
    ///
    /// This function is the same as [RayTracing::albedo] but takes buffers
    /// instead
    ///
    /// Returns [None] if albedo buffer was not created by this device
    pub fn albedo_buffer(&mut self, albedo: impl Into<Buffer>) -> Option<&mut RayTracing<'a>> {
        let albedo = albedo.into();
        if !self.device.same_device_as_buf(&albedo) {
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

    /// Set custom trained model weights for the RT filter.
    ///
    /// The bytes are copied and kept alive by the filter until new weights are
    /// supplied or [RayTracing::clear_weights] is called.
    pub fn weights(&mut self, weights: &[u8]) -> &mut RayTracing<'a> {
        self.weights = Some(weights.to_vec());
        if self.weights_set.get() {
            let weights = self.weights.as_ref().expect("weights were just set");
            unsafe {
                oidnSetSharedFilterData(
                    self.handle,
                    b"weights\0" as *const _ as _,
                    weights.as_ptr() as *mut _,
                    weights.len(),
                );
            }
        }
        self
    }

    /// Clear any previously supplied custom model weights.
    pub fn clear_weights(&mut self) -> &mut RayTracing<'a> {
        if self.weights_set.get() {
            unsafe {
                oidnUnsetFilterData(self.handle, b"weights\0" as *const _ as _);
            }
            self.weights_set.set(false);
        }
        self.weights = None;
        self
    }

    fn buffer_size_mismatch(buffer: &Option<Buffer>, expected: usize) -> bool {
        buffer.as_ref().is_some_and(|b| b.size != expected)
    }

    /// sets the dimensions of the denoising image, if new width * new height
    /// does not equal old width * old height
    pub fn image_dimensions(&mut self, width: usize, height: usize) -> &mut RayTracing<'a> {
        let buffer_dims = 3 * width * height;

        if Self::buffer_size_mismatch(&self.albedo, buffer_dims) {
            self.albedo = None;
            unsafe {
                oidnUnsetFilterImage(self.handle, b"albedo\0" as *const _ as _);
            }
        }

        if Self::buffer_size_mismatch(&self.normal, buffer_dims) {
            self.normal = None;
            unsafe {
                oidnUnsetFilterImage(self.handle, b"normal\0" as *const _ as _);
            }
        }

        self.img_dims = (width, height, buffer_dims);
        self
    }

    pub fn filter(&self, color: &[f32], output: &mut [f32]) -> Result<(), Error> {
        self.execute_filter(Some(color), output)
    }

    pub fn filter_buffer(&self, color: &Buffer, output: &Buffer) -> Result<(), Error> {
        self.execute_filter_buffer(Some(color), output)
    }

    /// Starts filtering buffer-backed images asynchronously.
    ///
    /// The returned guard keeps the filter and buffers borrowed until the
    /// device has been synchronized. Dropping the guard synchronizes the
    /// device as a convenience, but leaking it prevents synchronization.
    ///
    /// # Safety
    ///
    /// The returned guard must not be leaked with mechanisms such as
    /// [`std::mem::forget`] or [`std::mem::ManuallyDrop`]. It must be waited
    /// or dropped before the filter or buffers are accessed, mutated, or
    /// released.
    pub unsafe fn filter_buffer_async<'filter>(
        &'filter mut self,
        color: &'filter Buffer,
        output: &'filter mut Buffer,
    ) -> Result<PendingFilter<'filter, 'a>, Error> {
        unsafe { self.execute_filter_buffer_async(Some(color), output) }
    }

    pub fn filter_in_place(&self, color: &mut [f32]) -> Result<(), Error> {
        self.execute_filter(None, color)
    }

    pub fn filter_in_place_buffer(&self, color: &Buffer) -> Result<(), Error> {
        self.execute_filter_buffer(None, color)
    }

    /// Starts in-place filtering on a buffer asynchronously.
    ///
    /// The returned guard keeps the filter and buffer borrowed until the device
    /// has been synchronized. Dropping the guard synchronizes the device as a
    /// convenience, but leaking it prevents synchronization.
    ///
    /// # Safety
    ///
    /// The returned guard must not be leaked with mechanisms such as
    /// [`std::mem::forget`] or [`std::mem::ManuallyDrop`]. It must be waited
    /// or dropped before the filter or buffer are accessed, mutated, or
    /// released.
    pub unsafe fn filter_in_place_buffer_async<'filter>(
        &'filter mut self,
        color: &'filter mut Buffer,
    ) -> Result<PendingFilter<'filter, 'a>, Error> {
        unsafe { self.execute_filter_buffer_async(None, color) }
    }

    fn execute_filter(&self, color: Option<&[f32]>, output: &mut [f32]) -> Result<(), Error> {
        let color = match color {
            None => None,
            Some(color) => Some(self.device.create_buffer(color)?),
        };
        let out = self.device.create_buffer(output)?;
        self.execute_filter_buffer(color.as_ref(), &out)?;
        out.read_to_slice(output)?;
        Ok(())
    }

    fn execute_filter_buffer(&self, color: Option<&Buffer>, output: &Buffer) -> Result<(), Error> {
        self.configure_filter_buffer(color, output)?;
        unsafe {
            oidnExecuteFilter(self.handle);
        }
        self.device.get_error()?;
        Ok(())
    }

    unsafe fn execute_filter_buffer_async<'filter>(
        &'filter mut self,
        color: Option<&'filter Buffer>,
        output: &'filter mut Buffer,
    ) -> Result<PendingFilter<'filter, 'a>, Error> {
        self.configure_filter_buffer(color, output)?;
        unsafe {
            oidnExecuteFilterAsync(self.handle);
        }
        self.device.get_error()?;
        Ok(PendingFilter {
            filter: self,
            _color: color,
            _output: output,
            complete: false,
        })
    }

    fn configure_filter_buffer(
        &self,
        color: Option<&Buffer>,
        output: &Buffer,
    ) -> Result<(), Error> {
        self.device.clear_error();

        if let Some(alb) = &self.albedo {
            if alb.size != self.img_dims.2 {
                return Err(Error::new(
                    ErrorKind::InvalidImageDimensions,
                    "albedo buffer size does not match the image dimensions",
                ));
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
                    return Err(Error::new(
                        ErrorKind::InvalidImageDimensions,
                        "normal buffer size does not match the image dimensions",
                    ));
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
                if !self.device.same_device_as_buf(color) {
                    return Err(Error::new(
                        ErrorKind::InvalidArgument,
                        "color buffer was not created by this device",
                    ));
                }
                if color.size != self.img_dims.2 {
                    return Err(Error::new(
                        ErrorKind::InvalidImageDimensions,
                        "color buffer size does not match the image dimensions",
                    ));
                }
                color
            }
            None => {
                if !self.device.same_device_as_buf(output) {
                    return Err(Error::new(
                        ErrorKind::InvalidArgument,
                        "output buffer was not created by this device",
                    ));
                }
                if output.size != self.img_dims.2 {
                    return Err(Error::new(
                        ErrorKind::InvalidImageDimensions,
                        "output buffer size does not match the image dimensions",
                    ));
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
        if !self.device.same_device_as_buf(output) {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "output buffer was not created by this device",
            ));
        }
        if output.size != self.img_dims.2 {
            return Err(Error::new(
                ErrorKind::InvalidImageDimensions,
                "output buffer size does not match the image dimensions",
            ));
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
            if let Some(weights) = &self.weights {
                oidnSetSharedFilterData(
                    self.handle,
                    b"weights\0" as *const _ as _,
                    weights.as_ptr() as *mut _,
                    weights.len(),
                );
                self.weights_set.set(true);
            }

            oidnSetFilterInt(
                self.handle,
                b"quality\0" as *const _ as _,
                self.filter_quality,
            );

            oidnCommitFilter(self.handle);
        }

        self.device.get_error()?;

        Ok(())
    }
}

impl Drop for RayTracing<'_> {
    fn drop(&mut self) {
        unsafe {
            oidnReleaseFilter(self.handle);
            oidnReleaseDevice(self.device.0);
        }
    }
}

unsafe impl Send for RayTracing<'_> {}

/// Completion guard for an asynchronous OIDN filter execution.
///
/// Calling [`PendingFilter::wait`] or dropping this guard blocks until
/// [`Device::sync`] completes.
#[must_use = "dropping the guard blocks to synchronize; call wait() explicitly when possible"]
pub struct PendingFilter<'filter, 'device> {
    filter: &'filter mut RayTracing<'device>,
    _color: Option<&'filter Buffer>,
    _output: &'filter mut Buffer,
    complete: bool,
}

impl PendingFilter<'_, '_> {
    /// Blocks until the asynchronous filter execution has completed.
    pub fn wait(mut self) {
        self.finish();
    }

    fn finish(&mut self) {
        if !self.complete {
            self.filter.device.sync();
            self.complete = true;
        }
    }
}

impl Drop for PendingFilter<'_, '_> {
    fn drop(&mut self) {
        self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_replaces_and_clears_existing_shared_weights() {
        let device = Device::cpu();
        let mut filter = RayTracing::try_new(&device).unwrap();

        // Simulate the state after configure_filter_buffer has installed
        // weights, so the `weights_set == true` branches are covered without
        // needing valid OIDN model weights or a denoise operation.
        filter.weights_set.set(true);

        filter.weights(&[1, 2, 3, 4]);
        assert_eq!(filter.weights.as_deref(), Some(&[1, 2, 3, 4][..]));
        assert!(filter.weights_set.get());

        filter.clear_weights();
        assert!(filter.weights.is_none());
        assert!(!filter.weights_set.get());

        if let Err(err) = device.get_error() {
            panic!("OIDN device error after weights test: {err}");
        }
    }
}
