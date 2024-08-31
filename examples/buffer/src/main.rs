extern crate rand;

use rand::Rng;

const WIDTH: usize = 128;
const HEIGHT: usize = 9;
const BUFFER_LEN: usize = WIDTH * HEIGHT * 3;

fn main() {
    let mut input = [0.0; BUFFER_LEN];
    let mut rng = rand::thread_rng();
    for float in input.iter_mut() {
        let rand = rng.gen();
        *float = rand;
    }
    println!("randomized:");
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let idx = ((y * WIDTH) + x) * 3;
            let colour = &input[idx..idx + 3];
            print!(
                "\x1b[38;2;{};{};{}m#\x1b[0m",
                (colour[0] * 255.0) as u8,
                (colour[1] * 255.0) as u8,
                (colour[2] * 255.0) as u8
            )
        }
        println!();
    }
    let device = oidn::Device::new();
    let mut filter = oidn::filter::RayTracing::new(&device);
    let buffer = device.create_buffer(&input).unwrap();
    let mut output_buffer = device.create_buffer(&[0.0; BUFFER_LEN]).unwrap();
    filter
        .image_dimensions(WIDTH, HEIGHT)
        .filter_buffer(&buffer, &mut output_buffer)
        .unwrap();
    let slice = output_buffer.read();
    println!();
    println!("denoised:");
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let idx = ((y * WIDTH) + x) * 3;
            let colour = &slice[idx..idx + 3];
            print!(
                "\x1b[38;2;{};{};{}m#\x1b[0m",
                (colour[0] * 255.0) as u8,
                (colour[1] * 255.0) as u8,
                (colour[2] * 255.0) as u8
            )
        }
        println!();
    }
}
