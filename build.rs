#[cfg(not(feature = "bundled"))]
use pkg_config::Config;
use std::env;
use std::path::PathBuf;

fn main() {
    if env::var("DOCS_RS").is_ok() {
        return;
    }

    #[cfg(feature = "bundled")]
    configure_bundled();

    #[cfg(not(feature = "bundled"))]
    configure_system();
}

#[cfg(not(feature = "bundled"))]
fn configure_system() {
    if let Ok(dir) = env::var("OIDN_DIR") {
        let mut lib_path = PathBuf::from(dir);
        lib_path.push("lib");
        println!("cargo:rustc-link-search=native={}", lib_path.display());
    } else {
        Config::new().probe("OpenImageDenoise").unwrap_or_else(|e| {
            println!(
                "cargo:error=Could not find OpenImageDenoise via pkg-config: {}",
                e
            );
            panic!("Failed to find OpenImageDenoise");
        });
    }

    println!("cargo:rerun-if-env-changed=OIDN_DIR");
    println!("cargo:rustc-link-lib=OpenImageDenoise");
}

#[cfg(feature = "bundled")]
fn configure_bundled() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(PathBuf::from)
        .expect("could not determine workspace root");

    let bundled = build_tools::find_bundled_oidn_dir(&root)
        .or_else(|| build_tools::download_and_extract_oidn(&root).ok())
        .expect("failed to find or download bundled OpenImageDenoise package");

    let mut lib_path = bundled.clone();
    lib_path.push("lib");

    println!("cargo:rerun-if-env-changed=OIDN_BUNDLED_DIR");
    println!("cargo:rustc-link-search=native={}", lib_path.display());
    println!("cargo:rustc-link-lib=OpenImageDenoise");

    copy_runtime_libraries(&bundled);

    if env::consts::OS == "linux" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    } else if env::consts::OS == "macos" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
    }
}

#[cfg(feature = "bundled")]
fn copy_runtime_libraries(oidn_dir: &std::path::Path) {
    use std::fs;
    use std::path::Path;

    let Some(out_dir) = env::var_os("OUT_DIR").map(PathBuf::from) else {
        return;
    };

    let Some(profile_dir) = out_dir.ancestors().nth(3).map(Path::to_path_buf) else {
        return;
    };

    let runtime_dir = if env::consts::OS == "windows" {
        oidn_dir.join("bin")
    } else {
        oidn_dir.join("lib")
    };

    let destinations = [
        profile_dir.clone(),
        profile_dir.join("deps"),
        profile_dir.join("examples"),
    ];

    for destination in destinations {
        let _ = fs::create_dir_all(&destination);

        let Ok(entries) = fs::read_dir(&runtime_dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if !is_runtime_library(&path) {
                continue;
            }

            if let Some(file_name) = path.file_name() {
                let _ = fs::copy(&path, destination.join(file_name));
            }
        }
    }
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
