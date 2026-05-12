# `oidn`

[![Crates.io](https://img.shields.io/crates/v/oidn.svg)](https://crates.io/crates/oidn)
[![CI](https://github.com/Twinklebear/oidn-rs/actions/workflows/main.yml/badge.svg)](https://github.com/Twinklebear/oidn-rs/actions/workflows/main.yml)

Rust bindings to Intel's [Open Image Denoise library](https://github.com/OpenImageDenoise/oidn).
Crate version numbers track the OIDN version they correspond to.

## Documentation

Rust docs can be found [here](https://docs.rs/oidn).

Open Image Denoise documentation can be found [here](https://openimagedenoise.github.io/documentation.html).

## Bundled OIDN binaries

By default this crate links against an Open Image Denoise installation found
through `OIDN_DIR` or `pkg-config`. Enable the `bundled` feature to have the
build script download the matching official Open Image Denoise binary package
and link against it:

```toml
oidn = { version = "2.4.1", features = ["bundled"] }
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

## Development tasks

Repository maintenance commands live in the Rust `xtask` tool instead of
platform-specific shell scripts:

```bash
cargo run -p xtask -- build-examples
cargo run -p xtask -- build-test
cargo run -p xtask -- generate-sys-bindings
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
SHA-256 values in `build.rs`. The bundled CI job verifies the host archive
against the pinned checksum.

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

## Web demo

The [web demo](examples/web/) is a Rust/Yew browser gallery with checked-in
noisy and `oidn-rs` denoised WebP pairs. The GitHub Pages workflow builds the
demo, builds crate docs with `cargo doc --no-deps`, and publishes the docs under
`/docs/` in the same Pages artifact.

Preview the demo locally with:

```bash
cd examples/web
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
trunk serve --address 127.0.0.1 --port 8000 --no-spa --skip-version-check
```

Build the Pages artifact locally with:

```bash
cd examples/web
trunk build --release --public-url ./ --dist ../../target/pages --skip-version-check
cd ../..
cargo doc -p oidn --features bundled --no-deps
mkdir -p target/pages/docs
cp -R target/doc/. target/pages/docs/
```

Asset source notes live in
[examples/web/assets/README.md](examples/web/assets/README.md).
