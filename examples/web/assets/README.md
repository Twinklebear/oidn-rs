# Web Demo Assets

The `*-noisy.webp` and `*-denoised.webp` pairs are compact browser demo assets.
The denoised companions were generated with the crate's examples using Open
Image Denoise.

Dataset-backed scenes:

- `cornell-box-*`: based on the Mitsuba 3 Gallery `cornell-box` preview image.
- `teapot-*`: based on the Mitsuba 3 Gallery `teapot` preview image.
- `veach-ajar-*`: based on the Mitsuba 3 Gallery `veach-ajar` preview image.
- `exr-still-life-*`: based on OpenEXR `ScanLines/StillLife.exr`.
- `exr-mt-tam-west-*`: based on OpenEXR `ScanLines/MtTamWest.exr`.

Source references:

- https://mitsuba.readthedocs.io/en/stable/src/gallery.html
- https://benedikt-bitterli.me/resources/
- https://github.com/AcademySoftwareFoundation/openexr-images

For the web demo, source images were resized to at most 720 pixels on the long
edge, given deterministic render-like noise, filtered through `oidn-rs`, and
encoded to WebP for browser display.
