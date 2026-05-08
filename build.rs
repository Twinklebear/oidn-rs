use std::env;
use std::path::{Path, PathBuf};

#[cfg(not(feature = "bundled"))]
use pkg_config::Config;
#[cfg(feature = "bundled")]
use sha2::{Digest, Sha256};

fn main() {
    if env::var("DOCS_RS").is_ok() {
        return;
    }

    println!("cargo:rerun-if-env-changed=OIDN_BUNDLED_DIR");
    println!("cargo:rerun-if-env-changed=OIDN_DIR");

    #[cfg(feature = "bundled")]
    link_bundled_oidn().unwrap_or_else(|e| {
        println!("cargo:error=Could not prepare bundled OpenImageDenoise: {e}");
        panic!("Failed to prepare bundled OpenImageDenoise");
    });

    #[cfg(not(feature = "bundled"))]
    link_system_oidn();

    println!("cargo:rustc-link-lib=OpenImageDenoise");
}

#[cfg(not(feature = "bundled"))]
fn link_system_oidn() {
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
}

#[cfg(feature = "bundled")]
fn link_bundled_oidn() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(dir) = env::var("OIDN_BUNDLED_DIR") {
        let Some(lib_path) = find_oidn_lib_dir(&dir) else {
            return Err(format!(
                "OIDN_BUNDLED_DIR was set to `{dir}`, but no OpenImageDenoise library was found in that directory or its `lib` subdirectory"
            )
            .into());
        };
        let install_dir = bundled_install_dir(Path::new(&dir), &lib_path);
        link_bundled_lib_dir(&lib_path, &install_dir)?;
        return Ok(());
    }

    let package = BundledPackage::for_target()?;
    let install_dir = prepare_bundled_package(&package)?;
    let lib_path = install_dir.join("lib");

    if !contains_oidn_library(&lib_path) {
        return Err(format!(
            "OpenImageDenoise library was not found in extracted bundle `{}`",
            lib_path.display()
        )
        .into());
    }

    link_bundled_lib_dir(&lib_path, &install_dir)?;

    Ok(())
}

#[cfg(feature = "bundled")]
fn link_bundled_lib_dir(
    lib_path: &Path,
    install_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rustc-link-search=native={}", lib_path.display());
    add_relative_runtime_search_path();
    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => copy_runtime_libraries(&install_dir.join("bin"))?,
        Ok("linux") | Ok("macos") => copy_runtime_libraries(lib_path)?,
        _ => {}
    }
    Ok(())
}

#[cfg(feature = "bundled")]
fn bundled_install_dir(input_dir: &Path, lib_path: &Path) -> PathBuf {
    if input_dir.join("bin").is_dir() {
        input_dir.to_path_buf()
    } else {
        lib_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| input_dir.to_path_buf())
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

#[cfg(feature = "bundled")]
#[derive(Clone, Copy)]
enum ArchiveKind {
    TarGz,
    Zip,
}

#[cfg(feature = "bundled")]
struct BundledPackage {
    archive_name: String,
    install_dir: String,
    archive_kind: ArchiveKind,
    expected_sha256: &'static str,
}

#[cfg(feature = "bundled")]
impl BundledPackage {
    fn for_target() -> Result<Self, Box<dyn std::error::Error>> {
        let version = env!("CARGO_PKG_VERSION");
        let os = env::var("CARGO_CFG_TARGET_OS")?;
        let arch = env::var("CARGO_CFG_TARGET_ARCH")?;
        let platform = match (os.as_str(), arch.as_str()) {
            ("linux", "x86_64") => (
                "x86_64.linux",
                ArchiveKind::TarGz,
                "b69ca2443a226ef692ca46bdc4f89995b1e99091f2665b906cbe07e9673e48cc",
            ),
            ("macos", "x86_64") => (
                "x86_64.macos",
                ArchiveKind::TarGz,
                "b9addf2855ee36d7768fd02d4d540e64612096487bad302e608d04e639ae1584",
            ),
            ("macos", "aarch64") => (
                "arm64.macos",
                ArchiveKind::TarGz,
                "f1d7370bc09242bbd72d405b424ba240fd4d64103f3e607cdfeeaa2f2718cfb8",
            ),
            ("windows", "x86_64") => (
                "x64.windows",
                ArchiveKind::Zip,
                "682d94ba57525ed177d73412e0ed903f576867bd048f830a5c6f63c56b25e8b8",
            ),
            _ => {
                return Err(format!(
                    "the bundled feature does not have an OpenImageDenoise {version} package for target {arch}-{os}"
                )
                .into());
            }
        };
        let extension = match platform.1 {
            ArchiveKind::TarGz => "tar.gz",
            ArchiveKind::Zip => "zip",
        };
        let package_name = format!("oidn-{version}.{}", platform.0);

        Ok(Self {
            archive_name: format!("{package_name}.{extension}"),
            install_dir: package_name,
            archive_kind: platform.1,
            expected_sha256: platform.2,
        })
    }

    fn url(&self) -> String {
        format!(
            "https://github.com/OpenImageDenoise/oidn/releases/download/v{}/{}",
            env!("CARGO_PKG_VERSION"),
            self.archive_name
        )
    }
}

#[cfg(feature = "bundled")]
fn prepare_bundled_package(
    package: &BundledPackage,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let bundle_dir = out_dir.join("oidn-bundled");
    let install_dir = bundle_dir.join(&package.install_dir);

    if has_verified_bundle(&install_dir, package.expected_sha256) {
        return Ok(install_dir);
    }

    std::fs::create_dir_all(&bundle_dir)?;

    let archive_path = bundle_dir.join(&package.archive_name);
    if !archive_path.exists() {
        download_file(&package.url(), &archive_path, package.expected_sha256)?;
    } else {
        verify_file_sha256(&archive_path, package.expected_sha256)?;
    }

    if install_dir.exists() {
        std::fs::remove_dir_all(&install_dir)?;
    }

    extract_archive(&archive_path, &bundle_dir, package.archive_kind)?;
    write_checksum_marker(&install_dir, package.expected_sha256)?;
    Ok(install_dir)
}

#[cfg(feature = "bundled")]
fn download_file(
    url: &str,
    destination: &Path,
    expected_sha256: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let tmp_destination = destination.with_extension("download");
    let mut response = ureq::get(url).call()?;
    let mut reader = response.body_mut().as_reader();
    let mut file = std::fs::File::create(&tmp_destination)?;
    std::io::copy(&mut reader, &mut file)?;
    drop(file);
    if let Err(error) = verify_file_sha256(&tmp_destination, expected_sha256) {
        let _ = std::fs::remove_file(&tmp_destination);
        return Err(error);
    }
    std::fs::rename(tmp_destination, destination)?;
    Ok(())
}

#[cfg(feature = "bundled")]
fn verify_file_sha256(
    path: &Path,
    expected_sha256: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    let actual_sha256 = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    if actual_sha256.eq_ignore_ascii_case(expected_sha256) {
        Ok(())
    } else {
        Err(format!(
            "SHA256 mismatch for `{}`: expected {expected_sha256}, got {actual_sha256}",
            path.display()
        )
        .into())
    }
}

#[cfg(feature = "bundled")]
fn has_verified_bundle(install_dir: &Path, expected_sha256: &str) -> bool {
    contains_oidn_library(&install_dir.join("lib"))
        && std::fs::read_to_string(checksum_marker_path(install_dir))
            .is_ok_and(|checksum| checksum.trim().eq_ignore_ascii_case(expected_sha256))
}

#[cfg(feature = "bundled")]
fn write_checksum_marker(
    install_dir: &Path,
    expected_sha256: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(checksum_marker_path(install_dir), expected_sha256)?;
    Ok(())
}

#[cfg(feature = "bundled")]
fn checksum_marker_path(install_dir: &Path) -> PathBuf {
    install_dir.join(".oidn-rs-bundle.sha256")
}

#[cfg(feature = "bundled")]
fn extract_archive(
    archive_path: &Path,
    destination: &Path,
    archive_kind: ArchiveKind,
) -> Result<(), Box<dyn std::error::Error>> {
    match archive_kind {
        ArchiveKind::TarGz => {
            let file = std::fs::File::open(archive_path)?;
            let gz = flate2::read::GzDecoder::new(file);
            tar::Archive::new(gz).unpack(destination)?;
        }
        ArchiveKind::Zip => {
            let file = std::fs::File::open(archive_path)?;
            zip::ZipArchive::new(file)?.extract(destination)?;
        }
    }
    Ok(())
}

#[cfg(feature = "bundled")]
fn add_relative_runtime_search_path() {
    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") => println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN"),
        Ok("macos") => println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path"),
        _ => {}
    }
}

#[cfg(feature = "bundled")]
fn copy_runtime_libraries(source_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let libraries = runtime_libraries(source_dir)?;
    if libraries.is_empty() {
        return Err(format!(
            "no bundled OpenImageDenoise runtime libraries were found in `{}`",
            source_dir.display()
        )
        .into());
    }

    let profile_dir = target_profile_dir()?;
    for destination in [
        profile_dir.clone(),
        profile_dir.join("deps"),
        profile_dir.join("examples"),
    ] {
        std::fs::create_dir_all(&destination)?;
        for library in &libraries {
            let file_name = library.file_name().ok_or_else(|| {
                format!(
                    "could not determine bundled runtime library name for `{}`",
                    library.display()
                )
            })?;
            std::fs::copy(library, destination.join(file_name))?;
            println!("cargo:rerun-if-changed={}", library.display());
        }
    }

    Ok(())
}

#[cfg(feature = "bundled")]
fn runtime_libraries(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut libraries = Vec::new();
    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    for entry in dir.read_dir()? {
        let path = entry?.path();
        if is_runtime_library(&path, &target_os) {
            libraries.push(path);
        }
    }
    Ok(libraries)
}

#[cfg(feature = "bundled")]
fn is_runtime_library(path: &Path, target_os: &str) -> bool {
    let Some(file_name) = path.file_name().and_then(|file_name| file_name.to_str()) else {
        return false;
    };

    match target_os {
        "linux" => file_name.ends_with(".so") || file_name.contains(".so."),
        "macos" => file_name.ends_with(".dylib"),
        "windows" => path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dll")),
        _ => false,
    }
}

#[cfg(feature = "bundled")]
fn target_profile_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    out_dir
        .ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            format!(
                "could not determine Cargo target profile directory from OUT_DIR `{}`",
                out_dir.display()
            )
            .into()
        })
}
