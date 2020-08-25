# oidn-rs

[![Crates.io](https://img.shields.io/crates/v/oidn.svg)](https://crates.io/crates/oidn)
[![Build Status](https://travis-ci.org/Twinklebear/oidn-rs.svg?branch=master)](https://travis-ci.org/Twinklebear/oidn-rs)

Rust bindings to Intel's [OpenImageDenoise library](https://github.com/OpenImageDenoise/oidn).

# Documentation

Rust doc can be found [here](http://www.willusher.io/oidn-rs/oidn),
OpenImageDenoise documentation can be found [here](https://openimagedenoise.github.io/documentation.html).

## Example

oidn-rs provides a lightweight wrapper over the OpenImageDenoise library, along
with raw C bindings exposed under `oidn::sys`. Below is an example of using the
`RT` filter from OpenImageDenoise (the `RayTracing` filter) to denoise an image.

```rust
extern crate oidn;

fn main() {
    // Load scene, render image, etc.

    let input_img: Vec<f32> = // A float3 RGB image produced by your renderer
    let mut filter_output = vec![0.0f32; input_img.len()];

    let device = oidn::Device::new();
    let mut filter = oidn::RayTracing::new(&device);
    // Optionally add float3 normal and albedo buffers as well
    filter.set_srgb(true)
        .set_img_dims(input.width() as usize, input.height() as usize);
    filter.execute(&input_img[..], &mut filter_output[..]).expect("Filter config error!");

    if let Err(e) = device.get_error() {
        println!("Error denosing image: {}", e.1);
    }

    // Save out or display filter_output image
}
```

The [simple](examples/simple) example loads a JPG, denoises it, and saves the output image to a JPG.
The [denoise_exr](examples/denoise_exr) loads an HDR color EXR file, denoises it and saves the tonemapped
result out to a JPG. `denoise_exr` can also take albedo and normal data through additional EXR files.

