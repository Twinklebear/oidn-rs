extern crate docopt;
extern crate exr;
extern crate image;
extern crate oidn;
extern crate serde;

use docopt::Docopt;
use exr::prelude::rgba_image as rgb_exr;
use serde::Deserialize;
use std::f32;

/// An example application that shows opening an HDR EXR image with optional
/// additional normal and albedo EXR images and denoising it with OIDN.
/// The denoised image is then tonemaped and saved out as a JPG

const USAGE: &'static str = "
denoise_exr

Usage:
    denoise_exr -c <color.exr> -o <output.jpg> -e <exposure> [-a <albedo.exr>]
    denoise_exr -c <color.exr> -o <output.jpg> -e <exposure> [(-a <albedo.exr> -n <normal.exr>)]

Options:
    -c <color.exr>, --color <color.exr>     Specify the input color image
    -o <out.jpg>                            Specify the output file for the denoised and tonemapped JPG
    -e <exposure>, --exposure <exposure>    Specify the exposure to apply to the image
    -a <albedo.exr>, --albedo <albedo.exr>  Specify the albedo image
    -n <normal.exr>, --normal <normal.exr>  Specify the normal image (requires albedo)
";

#[derive(Debug, Deserialize)]
struct Args {
    flag_c: String,
    flag_o: String,
    flag_e: f32,
    flag_n: Option<String>,
    flag_a: Option<String>,
}

fn linear_to_srgb(x: f32) -> f32 {
    if x <= 0.0031308 {
        12.92 * x
    } else {
        1.055 * f32::powf(x, 1.0 / 2.4) - 0.055
    }
}

fn tonemap_kernel(x: f32) -> f32 {
    let a = 0.22;
    let b = 0.30;
    let c = 0.10;
    let d = 0.20;
    let e = 0.01;
    let f = 0.30;
    ((x * (a * x + c * b) + d * e) / (x * (a * x + b) + d * f)) - e / f
}

fn tonemap(x: f32) -> f32 {
    let w = 11.2;
    let scale = 1.758141;
    tonemap_kernel(x * scale) / tonemap_kernel(w)
}

struct EXRData {
    img: Vec<f32>,
    width: usize,
    height: usize,
}

impl EXRData {
    fn new(width: usize, height: usize) -> EXRData {
        EXRData {
            img: vec![0f32; width * height * 3],
            width: width,
            height: height,
        }
    }

    fn set_pixel(&mut self, x: usize, y: usize, pixel: &rgb_exr::Pixel) {
        let i = (y * self.width + x) * 3;
        self.img[i] = pixel.red.to_f32();
        self.img[i + 1] = pixel.green.to_f32();
        self.img[i + 2] = pixel.blue.to_f32();
    }
}

/// Load an EXR file to an RGB f32 buffer
fn load_exr(file: &str) -> EXRData {
    let (_info, image) = rgb_exr::ImageInfo::read_pixels_from_file(
        file,
        rgb_exr::read_options::high(),
        |info: &rgb_exr::ImageInfo| -> EXRData {
            EXRData::new(info.resolution.width(), info.resolution.height())
        },
        // set each pixel in the png buffer from the exr file
        |image: &mut EXRData, pos: rgb_exr::Vec2<usize>, pixel: rgb_exr::Pixel| {
            image.set_pixel(pos.x(), pos.y(), &pixel);
        },
    )
    .unwrap();
    image
}

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    let color = load_exr(&args.flag_c);
    let mut filter_output = vec![0f32; color.img.len()];

    let device = oidn::Device::new();
    let mut filter = oidn::RayTracing::new(&device);
    filter
        .set_srgb(false)
        .set_hdr(true)
        .set_img_dims(color.width, color.height);

    if let Some(albedo_exr) = args.flag_a {
        let albedo = load_exr(&albedo_exr);

        if let Some(normal_exr) = args.flag_n {
            let normal = load_exr(&normal_exr);
            filter
                .execute_with_albedo_normal(
                    &color.img[..],
                    &albedo.img[..],
                    &normal.img[..],
                    &mut filter_output[..],
                )
                .expect("Invalid input image dimensions?");
        } else {
            filter
                .execute_with_albedo(&color.img[..], &albedo.img[..], &mut filter_output[..])
                .expect("Invalid input image dimensions?");
        }
    } else {
        filter
            .execute(&color.img[..], &mut filter_output[..])
            .expect("Invalid input image dimensions?");
    }

    if let Err(e) = device.get_error() {
        println!("Error denosing image: {}", e.1);
    }

    let mut output_img = vec![0u8; filter_output.len()];
    for i in 0..filter_output.len() {
        let p = linear_to_srgb(tonemap(filter_output[i] * args.flag_e)) * 255.0;
        if p < 0.0 {
            output_img[i] = 0;
        } else if p > 255.0 {
            output_img[i] = 255;
        } else {
            output_img[i] = p as u8;
        }
    }
    image::save_buffer(
        &args.flag_o,
        &output_img[..],
        color.width as u32,
        color.height as u32,
        image::RGB(8),
    )
    .expect("Failed to save output image");
}
