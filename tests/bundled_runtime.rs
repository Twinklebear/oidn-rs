#![cfg(feature = "bundled")]

use std::fs;

#[test]
fn bundled_runtime_libraries_are_copied_next_to_test_binary() {
    let exe = std::env::current_exe().expect("test executable path should be available");
    let exe_dir = exe
        .parent()
        .expect("test executable should have a parent directory");

    let runtime_libraries = fs::read_dir(exe_dir)
        .expect("test executable directory should be readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_oidn_runtime_library)
        })
        .collect::<Vec<_>>();

    assert!(
        !runtime_libraries.is_empty(),
        "expected bundled OpenImageDenoise runtime libraries next to {}, found none",
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
