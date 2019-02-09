extern crate oidn;
extern crate image;

use std::env;

/// A simple test application that shows opening a color image and passing
/// it to OIDN for denoising. The denoised image is then saved out.

fn main() {
    let args: Vec<_> = env::args().collect();
    let input = image::open(&args[1][..]).expect("Failed to open input image").to_rgb();

    // OIDN works on float images only, so convert this to a floating point image
    let mut input_img = vec![0.0f32; (3 * input.width() * input.height()) as usize];
    for y in 0..input.height() {
        for x in 0..input.width() {
            let p = input.get_pixel(x, y);
            for c in 0..3 {
                input_img[3 * ((y * input.width() + x) as usize) + c] = p[c] as f32 / 255.0;
            }
        }
    }

    println!("Image dims {}x{}", input.width(), input.height());

    let mut filter_output = vec![0.0f32; input_img.len()];

    let mut device = oidn::Device::new();
    let mut filter = oidn::RayTracing::new(&mut device);
    filter.set_srgb(true)
        .set_img_dims(input.width() as usize, input.height() as usize);
    filter.execute(&input_img[..], &mut filter_output[..]);

    if let Err(e) = device.get_error() {
        println!("Error denosing image: {}", e.1);
    }

    let mut output_img = vec![0u8; filter_output.len()];
    for i in 0..filter_output.len() {
        let p = filter_output[i] * 255.0;
        if p < 0.0 {
            output_img[i] = 0;
        } else if p > 255.0 {
            output_img[i] = 255;
        } else {
            output_img[i] = p as u8;
        }
    }

    image::save_buffer(&args[2][..], &output_img[..], input.width(), input.height(), image::RGB(8))
        .expect("Failed to save output image");
}

