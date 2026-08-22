extern crate docopt;
extern crate exr;
extern crate image;
extern crate oidn;
extern crate rayon;
extern crate serde;

use docopt::Docopt;
use exr::prelude::read_first_rgba_layer_from_file;
use rayon::prelude::*;
use serde::Deserialize;

/// An example application that shows opening an HDR EXR image with optional
/// additional normal and albedo EXR images and denoising it with OIDN.
/// The denoised image is then tonemaped and saved out as a JPG
const USAGE: &str = "
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
            width,
            height,
        }
    }
    fn set_pixel(&mut self, x: usize, y: usize, pixel: (f32, f32, f32, f32)) {
        let i = (y * self.width + x) * 3;
        self.img[i] = pixel.0;
        self.img[i + 1] = pixel.1;
        self.img[i + 2] = pixel.2;
    }
}

/// Load an EXR file to an RGB f32 buffer
fn load_exr(file: &str) -> EXRData {
    read_first_rgba_layer_from_file(
        file,
        |resolution, _| -> EXRData { EXRData::new(resolution.width(), resolution.height()) },
        |image: &mut EXRData, pos, pixel: (f32, f32, f32, f32)| {
            image.set_pixel(pos.x(), pos.y(), pixel);
        },
    )
    .unwrap()
    .layer_data
    .channel_data
    .pixels
}

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    let mut color = load_exr(&args.flag_c);

    let device = oidn::Device::new().expect("failed to create an OIDN device");

    let albedo: EXRData;
    let normal: EXRData;

    let mut denoiser = oidn::RayTracing::try_new(&device).expect("unable to create OIDN filter");
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

    let exposure = 2.0_f32.powf(args.flag_e);
    let output_img = (0..color.img.len())
        .into_par_iter()
        .map(|i| {
            let p = linear_to_srgb(tonemap(color.img[i] * exposure));
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
        image::ColorType::Rgb8,
    )
    .expect("Failed to save output image");
}
