#[cfg(feature = "bundled")]
use build_tools;
use pkg_config::Config;
use std::env;
use std::path::PathBuf;

fn main() {
    if env::var("DOCS_RS").is_err() {
        if let Ok(dir) = env::var("OIDN_DIR") {
            let mut lib_path = PathBuf::from(dir);
            lib_path.push("lib");
            println!("cargo:rustc-link-search=native={}", lib_path.display());
        } else {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .map(PathBuf::from)
                .expect("could not determine workspace root");

            if let Some(bundled) = try_bundled_oidn_dir(&root) {
                let mut lib_path = bundled;
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
        }
        println!("cargo:rerun-if-env-changed=OIDN_DIR");
        println!("cargo:rustc-link-lib=OpenImageDenoise");
    }
}

#[cfg(feature = "bundled")]
fn try_bundled_oidn_dir(root: &PathBuf) -> Option<PathBuf> {
    build_tools::find_bundled_oidn_dir(root)
        .or_else(|| build_tools::download_and_extract_oidn(root).ok())
}

#[cfg(not(feature = "bundled"))]
fn try_bundled_oidn_dir(_root: &PathBuf) -> Option<PathBuf> {
    None
}
