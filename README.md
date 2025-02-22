# `oidn`

[![Crates.io](https://img.shields.io/crates/v/oidn.svg)](https://crates.io/crates/oidn)
[![CI](https://github.com/Twinklebear/oidn-rs/actions/workflows/main.yml/badge.svg)](https://github.com/Twinklebear/oidn-rs/actions/workflows/main.yml)

Rust bindings to Intel’s [Open Image Denoise library](https://github.com/OpenImageDenoise/oidn).
Crate version numbers track the OIDN version they correspond to.

## Documentation

Rust docs can be found [here](https://docs.rs/oidn).

Open Image Denoise documentation can be found [here](https://openimagedenoise.github.io/documentation.html).

## Example

The crate provides a lightweight wrapper over the Open Image Denoise library,
along with raw C bindings exposed under `oidn::sys`. Below is an example of
using the `RT` filter from Open Image Denoise (the `RayTracing` filter) to
denoise an image.

```rust
extern crate oidn;

fn main() {
    // Load scene, render image, etc.

    let input_img: Vec<f32> = // A float3 RGB image produced by your renderer
    let mut filter_output = vec![0.0f32; input_img.len()];

    let device = oidn::Device::new();
    oidn::RayTracing::new(&device)
        // Optionally add float3 normal and albedo buffers as well
        .srgb(true)
        .image_dimensions(input.width() as usize, input.height() as usize);
        .filter(&input_img[..], &mut filter_output[..])
        .expect("Filter config error!");

    if let Err(e) = device.get_error() {
        println!("Error denosing image: {}", e.1);
    }

    // Save out or display filter_output image
}
```

The [simple](examples/simple.rs) example loads a JPG, denoises it, and saves the
output image to a JPG. The [denoise_exr](examples/denoise_exr.rs) example loads an
HDR color EXR file, denoises it and saves the tonemapped result out to a JPG.
The `denoise_exr` app can also take albedo and normal data through additional
EXR files.
