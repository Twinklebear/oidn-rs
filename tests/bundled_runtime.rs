#![cfg(feature = "bundled")]

use std::fs;

#[test]
fn bundled_runtime_libraries_are_copied_into_the_build_tree() {
    let exe = std::env::current_exe().expect("test executable path should be available");

    // The build script copies the libraries into the profile directory, its
    // `deps` and its `examples`. Where the test binary itself ends up relative
    // to those depends on the cargo version, so search upwards from it rather
    // than assuming a layout.
    let runtime_libraries = exe
        .ancestors()
        .skip(1)
        .take(6)
        .map(runtime_libraries_in)
        .find(|libraries| !libraries.is_empty())
        .unwrap_or_default();

    assert!(
        !runtime_libraries.is_empty(),
        "expected bundled OpenImageDenoise runtime libraries at or above {}, found none",
        exe.display()
    );

    for library in runtime_libraries {
        assert!(
            library.metadata().is_ok_and(|metadata| metadata.len() > 0),
            "bundled runtime library should not be empty: {}",
            library.display()
        );
    }
}

fn runtime_libraries_in(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_oidn_runtime_library)
        })
        .collect()
}

fn is_oidn_runtime_library(file_name: &str) -> bool {
    if !file_name.contains("OpenImageDenoise") {
        return false;
    }

    if cfg!(target_os = "windows") {
        file_name.ends_with(".dll")
    } else if cfg!(target_os = "macos") {
        file_name.ends_with(".dylib")
    } else if cfg!(target_os = "linux") {
        file_name.ends_with(".so") || file_name.contains(".so.")
    } else {
        false
    }
}
