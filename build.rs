use std::env;
use std::path::{Path, PathBuf};

use pkg_config::Config;

fn main() {
    if env::var("DOCS_RS").is_err() {
        let oidn_dir = env::var("OIDN_DIR").ok();
        if let Some(lib_path) = oidn_dir.as_deref().and_then(find_oidn_lib_dir) {
            println!("cargo:rustc-link-search=native={}", lib_path.display());
        } else if let Err(e) = Config::new().probe("OpenImageDenoise") {
            if let Some(dir) = oidn_dir {
                println!(
                    "cargo:warning=OIDN_DIR was set to `{dir}`, but no OpenImageDenoise library was found in that directory or its `lib` subdirectory."
                );
            }
            println!("cargo:error=Could not find OpenImageDenoise via pkg-config: {e}");
            panic!("Failed to find OpenImageDenoise");
        }
        println!("cargo:rerun-if-env-changed=OIDN_DIR");
        println!("cargo:rustc-link-lib=OpenImageDenoise");
    }
}

fn find_oidn_lib_dir(dir: &str) -> Option<PathBuf> {
    let root = PathBuf::from(dir);
    let lib = root.join("lib");
    if contains_oidn_library(&lib) {
        Some(lib)
    } else if contains_oidn_library(&root) {
        Some(root)
    } else {
        None
    }
}

fn contains_oidn_library(path: &Path) -> bool {
    let Ok(entries) = path.read_dir() else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        name == "OpenImageDenoise.lib"
            || name == "libOpenImageDenoise.so"
            || name.starts_with("libOpenImageDenoise.so.")
            || name == "libOpenImageDenoise.dylib"
            || name.starts_with("libOpenImageDenoise.") && name.ends_with(".dylib")
    })
}
