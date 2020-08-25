use std::env;
use std::path::PathBuf;

fn main() {
    if let Ok(e) = env::var("OIDN_DIR") {
        let mut oidn_dir = PathBuf::from(e);
        oidn_dir.push("lib");
        println!("cargo:rustc-link-search=native={}", oidn_dir.display());
    } else {
        println!("cargo:error=Please set OIDN_DIR=<path to OpenImageDenoise install root>");
        panic!("Failed to find OpenImageDenoise");
    }
    println!("cargo:rerun-if-env-changed=OIDN_DIR");
    println!("cargo:rustc-link-lib=OpenImageDenoise");
}
