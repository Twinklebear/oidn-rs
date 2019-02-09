extern crate oidn;
extern crate image;

use std::env;
use std::ptr;
use std::os::raw::{c_char, c_void};
use std::ffi::{CStr, CString};

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
    unsafe {
        let device = oidn::sys::oidnNewDevice(oidn::DeviceType::DEFAULT);
        oidn::sys::oidnCommitDevice(device);

        let filter_type = CString::new("RT").unwrap();
        let filter = oidn::sys::oidnNewFilter(device, filter_type.as_ptr());

        let color_buf_name = CString::new("color").unwrap();
        oidn::sys::oidnSetSharedFilterImage(filter, color_buf_name.as_ptr(),
                                            input_img.as_mut_ptr() as *mut c_void,
                                            oidn::Format::FLOAT3, input.width() as usize,
                                            input.height() as usize, 0, 0, 0);

        let output_buf_name = CString::new("output").unwrap();
        oidn::sys::oidnSetSharedFilterImage(filter, output_buf_name.as_ptr(),
                                            filter_output.as_mut_ptr() as *mut c_void,
                                            oidn::Format::FLOAT3, input.width() as usize,
                                            input.height() as usize, 0, 0, 0);

        let srgb_name = CString::new("srgb").unwrap();
        oidn::sys::oidnSetFilter1b(filter, srgb_name.as_ptr(), true);
        oidn::sys::oidnCommitFilter(filter);

        oidn::sys::oidnExecuteFilter(filter);
        let mut err_msg = ptr::null();
        if oidn::sys::oidnGetDeviceError(device, &mut err_msg as *mut *const c_char) != oidn::Error::NONE {
            let err_str = CStr::from_ptr(err_msg).to_string_lossy();
            println!("OIDN Error: {}", err_str);
        }

        oidn::sys::oidnReleaseFilter(filter);
        oidn::sys::oidnReleaseDevice(device);
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

