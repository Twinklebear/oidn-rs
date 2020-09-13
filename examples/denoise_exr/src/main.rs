extern crate docopt;
extern crate exr;
extern crate image;
extern crate oidn;
extern crate rayon;
extern crate serde;

use docopt::Docopt;
use exr::prelude::rgba_image as rgb_exr;
use rayon::prelude::*;
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

    let mut color = load_exr(&args.flag_c);

    let device = oidn::Device::new();

    let albedo: EXRData;
    let normal: EXRData;

    let mut denoiser = oidn::RayTracing::new(&device);
    denoiser
        .srgb(false)
        .hdr(true)
        .image_dimensions(color.width, color.height);

    if let Some(albedo_exr) = args.flag_a.clone() {
        albedo = load_exr(&albedo_exr);

        if let Some(normal_exr) = args.flag_n.clone() {
            normal = load_exr(&normal_exr);
            denoiser.albedo_normal(&albedo.img[..], &normal.img[..]);
        } else {
            denoiser.albedo(&albedo.img[..]);
        }
    }

    denoiser
        .filter_in_place(&mut color.img[..])
        .expect("Invalid input image dimensions?");

    if let Err(e) = device.get_error() {
        println!("Error denosing image: {}", e.1);
    }

    let output_img = (0..color.img.len())
        .into_par_iter()
        .map(|i| {
            let p = linear_to_srgb(tonemap(color.img[i] * args.flag_e));
            if p < 0.0 {
                0u8
            } else if p > 1.0 {
                255u8
            } else {
                (p * 255.0) as u8
            }
        })
        .collect::<Vec<_>>();

    image::save_buffer(
        &args.flag_o,
        &output_img[..],
        color.width as u32,
        color.height as u32,
        image::RGB(8),
    )
    .expect("Failed to save output image");
}
