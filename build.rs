use std::env;
use std::path::PathBuf;

fn main() {
    if env::var("DOCS_RS").is_err() {
        if let Ok(dir) = env::var("OIDN_DIR") {
            let mut lib_path = PathBuf::from(dir);
            lib_path.push("lib");
            println!("cargo:rustc-link-search=native={}", lib_path.display());
        } else {
            pkg_config::Config::new()
                .probe("OpenImageDenoise")
                .unwrap_or_else(|e| {
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
}
