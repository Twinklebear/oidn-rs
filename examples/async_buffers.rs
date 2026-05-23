extern crate oidn;

use oidn::{Device, Quality, RayTracing, Storage};

const WIDTH: usize = 2;
const HEIGHT: usize = 1;
const PIXELS: usize = WIDTH * HEIGHT * 3;

fn main() {
    let device = Device::cpu();
    let input = [0.18, 0.22, 0.27, 0.74, 0.68, 0.59];

    let mut color = device
        .create_buffer_with_storage(PIXELS, Storage::Host)
        .expect("failed to allocate input buffer");
    unsafe {
        device
            .write_buffer_async(&mut color, &input)
            .expect("input length should match the buffer")
            .wait();
    }

    let mut output = device
        .create_buffer_with_storage(PIXELS, Storage::Host)
        .expect("failed to allocate output buffer");

    let mut filter = RayTracing::new(&device);
    filter
        .filter_quality(Quality::Balanced)
        .srgb(true)
        .image_dimensions(WIDTH, HEIGHT);
    unsafe {
        filter
            .filter_buffer_async(&color, &mut output)
            .expect("buffer dimensions should match the filter")
            .wait();
    }

    let mut denoised = [0.0; PIXELS];
    unsafe {
        device
            .read_buffer_async(&mut output, &mut denoised)
            .expect("output length should match the buffer")
            .wait();
    }

    if let Err(error) = device.get_error() {
        panic!("Open Image Denoise failed: {}", error.1);
    }

    for rgb in denoised.chunks_exact(3) {
        println!("{:.4} {:.4} {:.4}", rgb[0], rgb[1], rgb[2]);
    }
}
