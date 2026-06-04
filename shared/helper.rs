pub fn oidn_dir(root: &std::path::Path) -> Option<std::path::PathBuf> {
    if let Some(dir) = non_empty_env_path("OIDN_DIR") {
        return Some(dir);
    }

    let version = std::env::var("OIDN_VERSION")
        .ok()
        .filter(|version| !version.is_empty())
        .or_else(|| package_version(root))?;

    oidn_package_dirs(root, &version)
        .into_iter()
        .find(|dir| dir.is_dir())
}

pub fn oidn_package_dirs(root: &std::path::Path, version: &str) -> Vec<std::path::PathBuf> {
    platform_package_suffixes()
        .iter()
        .map(|suffix| root.join(format!("oidn-{version}.{suffix}")))
        .collect()
}

pub fn non_empty_env_path(name: &str) -> Option<std::path::PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
}

#[cfg(feature = "bundled")]
pub fn download_and_extract_oidn(
    root: &std::path::Path,
) -> std::result::Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let version = std::env::var("OIDN_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| package_version(root))
        .ok_or("OIDN version could not be determined")?;

    let info = package_asset_info()?;
    let archive_name = format!("oidn-{version}.{}", info.archive_suffix);
    let archive_path = root.join(&archive_name);
    let package_dir = root.join(format!("oidn-{version}.{}", info.package_suffix));

    ensure_archive(
        &archive_path,
        &format!(
            "https://github.com/OpenImageDenoise/oidn/releases/download/v{version}/{archive_name}",
            version = version,
            archive_name = archive_name
        ),
        info.sha256,
    )?;

    if !package_dir.is_dir() {
        extract_archive(root, &archive_path)?;
    }

    Ok(package_dir)
}

pub fn find_bundled_oidn_dir(root: &std::path::Path) -> Option<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os("OIDN_BUNDLED_DIR")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .filter(|dir| dir.is_dir())
    {
        return Some(dir);
    }

    let version = package_version(root)?;
    let dirs = platform_package_suffixes()
        .iter()
        .map(|s| root.join(format!("oidn-{version}.{s}")));

    dirs.into_iter().find(|d| d.is_dir())
}

#[cfg(feature = "bundled")]
struct PackageAssetInfo {
    package_suffix: &'static str,
    archive_suffix: &'static str,
    sha256: &'static str,
}

#[cfg(feature = "bundled")]
fn ensure_archive(
    archive_path: &std::path::Path,
    url: &str,
    expected_sha: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    if archive_path.exists() {
        if verify_sha256(archive_path, expected_sha)? {
            println!("Using cached archive {}", archive_path.display());
            return Ok(());
        }
        std::fs::remove_file(archive_path)?;
    }

    download_archive(archive_path, url)?;
    if !verify_sha256(archive_path, expected_sha)? {
        return Err(format!(
            "downloaded archive {} failed SHA-256 verification",
            archive_path.display()
        )
        .into());
    }
    Ok(())
}

#[cfg(feature = "bundled")]
fn download_archive(
    archive_path: &std::path::Path,
    url: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("Downloading {}", url);
    let response = ureq::get(url).call()?;
    let mut reader = response.into_parts().1.into_reader();
    let mut file = std::fs::File::create(archive_path)?;
    std::io::copy(&mut reader, &mut file)?;
    Ok(())
}

#[cfg(feature = "bundled")]
fn verify_sha256(
    path: &std::path::Path,
    expected_hex: &str,
) -> std::result::Result<bool, Box<dyn std::error::Error>> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = std::io::Read::read(&mut file, &mut buffer)?;
        if count == 0 {
            break;
        }
        sha2::Digest::update(&mut hasher, &buffer[..count]);
    }
    let actual_hex = sha2::Digest::finalize(hasher)
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    Ok(actual_hex == expected_hex)
}

#[cfg(feature = "bundled")]
fn extract_archive(
    root: &std::path::Path,
    archive_path: &std::path::Path,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let name = archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("invalid archive name")?;

    if name.ends_with(".zip") {
        extract_zip_archive(root, archive_path)
    } else if name.ends_with(".tar.gz") {
        extract_tar_gz_archive(root, archive_path)
    } else {
        Err("unsupported archive format".into())
    }
}

#[cfg(feature = "bundled")]
fn extract_zip_archive(
    root: &std::path::Path,
    archive_path: &std::path::Path,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(enclosed_name) = entry
            .enclosed_name()
            .as_deref()
            .map(std::path::Path::to_path_buf)
        else {
            return Err(format!("zip archive contains unsafe path: {}", entry.name()).into());
        };

        let output_path = safe_archive_path(root, &enclosed_name)?;

        if entry.is_dir() {
            std::fs::create_dir_all(&output_path)?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut output = std::fs::File::create(&output_path)?;
        std::io::copy(&mut entry, &mut output)?;
    }

    Ok(())
}

#[cfg(feature = "bundled")]
fn extract_tar_gz_archive(
    root: &std::path::Path,
    archive_path: &std::path::Path,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::open(archive_path)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();
        let output_path = safe_archive_path(root, &entry_path)?;

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        entry.unpack(&output_path)?;
    }

    Ok(())
}

#[cfg(feature = "bundled")]
fn safe_archive_path(
    root: &std::path::Path,
    entry_name: &std::path::Path,
) -> std::result::Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    use std::path::Component;

    if entry_name.is_absolute() {
        return Err(format!("archive entry has absolute path: {}", entry_name.display()).into());
    }

    if entry_name.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::Prefix(_) | Component::RootDir
        )
    }) {
        return Err(format!(
            "archive entry escapes extraction directory: {}",
            entry_name.display()
        )
        .into());
    }

    Ok(root.join(entry_name))
}

pub fn package_version(root: &std::path::Path) -> Option<String> {
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).ok()?;
    let mut in_package = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if !in_package || !trimmed.starts_with("version") {
            continue;
        }
        let (_, value) = trimmed.split_once('=')?;
        return Some(value.trim().trim_matches('"').to_string());
    }
    None
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn platform_package_suffixes() -> &'static [&'static str] {
    &["x86_64.linux"]
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub fn platform_package_suffixes() -> &'static [&'static str] {
    &["arm64.macos"]
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub fn platform_package_suffixes() -> &'static [&'static str] {
    &["x86_64.macos"]
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub fn platform_package_suffixes() -> &'static [&'static str] {
    &["x64.windows"]
}

#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64"),
)))]
compile_error!("unsupported host platform for OIDN download");

#[cfg(feature = "bundled")]
fn package_asset_info() -> Result<PackageAssetInfo, Box<dyn std::error::Error>> {
    Ok(package_asset_info_for_target())
}

#[cfg(all(feature = "bundled", target_os = "linux", target_arch = "x86_64"))]
fn package_asset_info_for_target() -> PackageAssetInfo {
    PackageAssetInfo {
        package_suffix: "x86_64.linux",
        archive_suffix: "x86_64.linux.tar.gz",
        sha256: include_str!("../oidn_hashes/x86_64.linux.sha256").trim(),
    }
}

#[cfg(all(feature = "bundled", target_os = "macos", target_arch = "x86_64"))]
fn package_asset_info_for_target() -> PackageAssetInfo {
    PackageAssetInfo {
        package_suffix: "x86_64.macos",
        archive_suffix: "x86_64.macos.tar.gz",
        sha256: include_str!("../oidn_hashes/x86_64.macos.sha256").trim(),
    }
}

#[cfg(all(feature = "bundled", target_os = "macos", target_arch = "aarch64"))]
fn package_asset_info_for_target() -> PackageAssetInfo {
    PackageAssetInfo {
        package_suffix: "arm64.macos",
        archive_suffix: "arm64.macos.tar.gz",
        sha256: include_str!("../oidn_hashes/arm64.macos.sha256").trim(),
    }
}

#[cfg(all(feature = "bundled", target_os = "windows", target_arch = "x86_64"))]
fn package_asset_info_for_target() -> PackageAssetInfo {
    PackageAssetInfo {
        package_suffix: "x64.windows",
        archive_suffix: "x64.windows.zip",
        sha256: include_str!("../oidn_hashes/x64.windows.sha256").trim(),
    }
}

#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64"),
)))]
compile_error!("unsupported host platform for OIDN download");
