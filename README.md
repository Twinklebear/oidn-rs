# `oidn`

[![Crates.io](https://img.shields.io/crates/v/oidn.svg)](https://crates.io/crates/oidn)
[![CI](https://github.com/Twinklebear/oidn-rs/actions/workflows/main.yml/badge.svg)](https://github.com/Twinklebear/oidn-rs/actions/workflows/main.yml)

Rust bindings to Intel’s [Open Image Denoise library](https://github.com/OpenImageDenoise/oidn).
Crate version numbers track the OIDN version they correspond to.

## Documentation

Rust docs can be found [here](https://docs.rs/oidn).

Open Image Denoise documentation can be found [here](https://openimagedenoise.github.io/documentation.html).

## Development tasks

Repository maintenance commands live in the Rust `xtask` tool instead of
platform-specific shell scripts:

```bash
cargo run -p xtask -- build-examples
cargo run -p xtask -- build-test
cargo run -p xtask -- generate-sys-bindings
cargo run -p xtask -- download-oidn-package
cargo run -p xtask -- check-coverage
```

`build-test` uses `OIDN_DIR` when it is set. Otherwise it looks for an
extracted `oidn-<version>.<platform>` package in the repository root and sets
the host runtime library path before running Cargo. The binding generator
defaults to `src/sys.rs` and finds `oidn.h` from `OIDN_HEADER`, `OIDN_DIR`,
`OIDN_BUNDLED_DIR`, an extracted OIDN package in the repository root, or the
bundled package under `target`. Explicit header and output paths can also be
passed to `generate-sys-bindings`. The binding generator expects `bindgen` and
a usable `libclang` installation to be available.

When bumping the Open Image Denoise version, update the crate version,
`.github/workflows/main.yml`'s `OIDN_VERSION`, and the bundled package
SHA-256 values in `oidn_hashes`. The bundled CI job verifies the host archive
against the pinned checksum.

## Bundled OIDN binaries

By default this crate links against an Open Image Denoise installation found
through `OIDN_DIR` or `pkg-config`. Enable the `bundled` feature to have the
build script download the matching official Open Image Denoise binary package
and link against it:

```toml
oidn = { version = "2.5.0", features = ["bundled"] }
```

The bundled feature supports the official OIDN packages for x86_64 Linux,
x86_64 Windows, and x86_64/aarch64 macOS. Downloaded archives are verified
against pinned SHA-256 checksums for the crate's OIDN version. Set
`OIDN_BUNDLED_DIR` to a pre-extracted OIDN package root if you want to provide
the files yourself or avoid a network download during the build. The feature
still links the dynamic OIDN libraries, so applications that redistribute
binaries must also ship the runtime libraries from the bundled package's `lib`
or `bin` directory. For local `cargo run`, examples, and tests, the build
script copies bundled runtime libraries into the active Cargo target output
directories and uses relative runtime search paths on Linux and macOS.

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
    oidn::RayTracing::try_new(&device)
        .expect("Failed to create the filter")
        // Optionally add float3 normal and albedo buffers as well
        .srgb(true)
        .image_dimensions(input.width() as usize, input.height() as usize)
        .filter(&input_img[..], &mut filter_output[..])
        .expect("Filter config error!");

    // Save out or display filter_output image
}
```

The [simple](examples/simple.rs) example loads a JPG, denoises it, and saves the
output image to a JPG. The [denoise_exr](examples/denoise_exr.rs) example loads an
HDR color EXR file, denoises it and saves the tonemapped result out to a JPG.
The `denoise_exr` app can also take albedo and normal data through additional
EXR files.

## Graphics API interop

Buffers and semaphores can be imported from a graphics API, so that rendering
and denoising share the same memory instead of copying it through the host:

- `Device::create_shared_buffer_from_raw_fd` / `create_shared_buffer_from_raw_handle`
  import memory exported by the other API, and `Device::external_memory_types`
  reports the handle types the device accepts.
- `Device::create_shared_semaphore_from_raw_fd` / `create_shared_semaphore_from_raw_handle`
  import a semaphore or fence to synchronize access to that memory, with
  `Device::signal_semaphores_async` and `Device::wait_semaphores_async`, and
  `Device::external_semaphore_types` reports the handle types the device
  accepts.
- `Device::by_luid` and `Device::by_uuid` place the denoising device on the
  same physical device as the graphics API, which importing requires.

Open Image Denoise 2.5.0 supports importing external semaphores only on CUDA
(Windows and Linux) and HIP (Windows) devices, so both queries return empty on
CPU devices and an application always needs a fallback that copies through the
host and synchronizes with `Device::sync`.

Two Direct3D 12 round trips exercise this, one in a single process and one
across two, where the child opens the shared objects by Win32 object name.
They live in the `oidn-interop-tests` workspace member, so that the crate that
drives Direct3D 12 stays out of this crate's dependencies on every platform.
They need a GPU that can import Direct3D 12 resources and fences and skip
themselves when the adapter cannot:

```
cargo test -p oidn-interop-tests -- --nocapture
```

Note that the importing process must outlive the exporting device's use of the
shared resources. Tearing it down first removes that device, after which its
fences report `u64::MAX` and queued work is silently dropped.
