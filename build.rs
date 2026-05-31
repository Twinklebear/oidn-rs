#[cfg(not(feature = "bundled"))]
use pkg_config::Config;
use std::env;
use std::path::PathBuf;

fn main() {
    if env::var("DOCS_RS").is_ok() {
        return;
    }

    #[cfg(feature = "bundled")]
    configure_bundled().expect("failed to configure bundled OpenImageDenoise");

    #[cfg(not(feature = "bundled"))]
    configure_system();
}

#[cfg(not(feature = "bundled"))]
fn configure_system() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    if let Some(dir) = build_tools::oidn_dir(&root) {
        let lib_path = dir.join("lib");

        println!("cargo:rerun-if-env-changed=OIDN_DIR");
        println!("cargo:rerun-if-env-changed=OIDN_VERSION");
        println!(
            "cargo:rerun-if-changed={}",
            root.join("Cargo.toml").display()
        );
        println!("cargo:rerun-if-changed={}", dir.display());
        println!("cargo:rerun-if-changed={}", lib_path.display());

        println!("cargo:rustc-link-search=native={}", lib_path.display());
        println!("cargo:rustc-link-lib=OpenImageDenoise");
        return;
    }

    Config::new().probe("OpenImageDenoise").unwrap_or_else(|e| {
        println!(
            "cargo:error=Could not find OpenImageDenoise via OIDN_DIR, local extracted package, or pkg-config: {}",
            e
        );
        panic!("Failed to find OpenImageDenoise");
    });

    println!("cargo:rerun-if-env-changed=OIDN_DIR");
    println!("cargo:rerun-if-env-changed=OIDN_VERSION");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rustc-link-lib=OpenImageDenoise");
}

#[cfg(feature = "bundled")]
fn configure_bundled() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let bundled = match build_tools::find_bundled_oidn_dir(&root) {
        Some(dir) => dir,
        None => build_tools::download_and_extract_oidn(&root)?,
    };

    let lib_path = bundled.join("lib");
    let runtime_path = if env::consts::OS == "windows" {
        bundled.join("bin")
    } else {
        bundled.join("lib")
    };

    println!("cargo:rerun-if-env-changed=OIDN_BUNDLED_DIR");
    println!("cargo:rerun-if-env-changed=OIDN_VERSION");

    println!(
        "cargo:rerun-if-changed={}",
        root.join("Cargo.toml").display()
    ); //if the crate version changes download the new version of OIDN
    println!("cargo:rerun-if-changed={}", bundled.display());
    println!("cargo:rerun-if-changed={}", lib_path.display());
    println!("cargo:rerun-if-changed={}", runtime_path.display());

    println!("cargo:rustc-link-search=native={}", lib_path.display());
    println!("cargo:rustc-link-lib=OpenImageDenoise");

    copy_runtime_libraries(&bundled)?;

    if env::consts::OS == "linux" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    } else if env::consts::OS == "macos" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
    }

    Ok(())
}

#[cfg(feature = "bundled")]
fn copy_runtime_libraries(oidn_dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs;
    use std::path::Path;

    let Some(out_dir) = env::var_os("OUT_DIR").map(PathBuf::from) else {
        return Ok(());
    };

    let Some(profile_dir) = out_dir.ancestors().nth(3).map(Path::to_path_buf) else {
        return Ok(());
    };

    let runtime_dir = if env::consts::OS == "windows" {
        oidn_dir.join("bin")
    } else {
        oidn_dir.join("lib")
    };

    if !runtime_dir.is_dir() {
        return Err(format!(
            "bundled OpenImageDenoise runtime directory does not exist: {}",
            runtime_dir.display()
        )
        .into());
    }

    let destinations = [
        profile_dir.clone(),
        profile_dir.join("deps"),
        profile_dir.join("examples"),
    ];

    let runtime_libraries = fs::read_dir(&runtime_dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| is_runtime_library(path))
        .collect::<Vec<_>>();

    if runtime_libraries.is_empty() {
        return Err(format!(
            "no OpenImageDenoise runtime libraries found in {}",
            runtime_dir.display()
        )
        .into());
    }

    for destination in destinations {
        fs::create_dir_all(&destination)?;

        for library in &runtime_libraries {
            let Some(file_name) = library.file_name() else {
                continue;
            };

            fs::copy(library, destination.join(file_name)).map_err(|error| {
                format!(
                    "failed to copy runtime library {} to {}: {}",
                    library.display(),
                    destination.display(),
                    error
                )
            })?;
        }
    }

    Ok(())
}

#[cfg(feature = "bundled")]
fn is_runtime_library(path: &std::path::Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    match env::consts::OS {
        "windows" => file_name.ends_with(".dll"),
        "macos" => file_name.ends_with(".dylib"),
        "linux" => file_name.ends_with(".so") || file_name.contains(".so."),
        _ => false,
    }
}
