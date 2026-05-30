use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(feature = "bundled")]
use flate2::read::GzDecoder;
#[cfg(feature = "bundled")]
use sha2::{Digest, Sha256};
#[cfg(feature = "bundled")]
use std::io::{self, Read};
#[cfg(feature = "bundled")]
use tar::Archive;
#[cfg(feature = "bundled")]
use zip::ZipArchive;

type DynError = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, DynError>;

#[cfg(feature = "bundled")]
pub fn download_and_extract_oidn(root: &Path) -> Result<PathBuf> {
    let version = env::var("OIDN_VERSION")
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

#[cfg(feature = "bundled")]
pub fn find_bundled_oidn_dir(root: &Path) -> Option<PathBuf> {
    if let Some(dir) = env::var_os("OIDN_BUNDLED_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
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
fn package_asset_info() -> Result<PackageAssetInfo> {
    match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => Ok(PackageAssetInfo {
            package_suffix: "x86_64.linux",
            archive_suffix: "x86_64.linux.tar.gz",
            sha256: "b69ca2443a226ef692ca46bdc4f89995b1e99091f2665b906cbe07e9673e48cc",
        }),
        ("macos", "x86_64") => Ok(PackageAssetInfo {
            package_suffix: "x86_64.macos",
            archive_suffix: "x86_64.macos.tar.gz",
            sha256: "b9addf2855ee36d7768fd02d4d540e64612096487bad302e608d04e639ae1584",
        }),
        ("macos", "aarch64") => Ok(PackageAssetInfo {
            package_suffix: "arm64.macos",
            archive_suffix: "arm64.macos.tar.gz",
            sha256: "f1d7370bc09242bbd72d405b424ba240fd4d64103f3e607cdfeeaa2f2718cfb8",
        }),
        ("windows", "x86_64") => Ok(PackageAssetInfo {
            package_suffix: "x64.windows",
            archive_suffix: "x64.windows.zip",
            sha256: "682d94ba57525ed177d73412e0ed903f576867bd048f830a5c6f63c56b25e8b8",
        }),
        _ => Err("unsupported host platform for OIDN download".into()),
    }
}

#[cfg(feature = "bundled")]
struct PackageAssetInfo {
    package_suffix: &'static str,
    archive_suffix: &'static str,
    sha256: &'static str,
}

#[cfg(feature = "bundled")]
fn ensure_archive(archive_path: &Path, url: &str, expected_sha: &str) -> Result<()> {
    if archive_path.exists() {
        if verify_sha256(archive_path, expected_sha)? {
            println!("Using cached archive {}", archive_path.display());
            return Ok(());
        }
        fs::remove_file(archive_path)?;
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
fn download_archive(archive_path: &Path, url: &str) -> Result<()> {
    println!("Downloading {}", url);
    let response = ureq::get(url).call()?;
    let mut reader = response.into_parts().1.into_reader();
    let mut file = fs::File::create(archive_path)?;
    io::copy(&mut reader, &mut file)?;
    Ok(())
}

#[cfg(feature = "bundled")]
fn verify_sha256(path: &Path, expected_hex: &str) -> Result<bool> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let actual_hex = hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    Ok(actual_hex == expected_hex)
}

#[cfg(feature = "bundled")]
fn extract_archive(root: &Path, archive_path: &Path) -> Result<()> {
    let name = archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("invalid archive name")?;
    if name.ends_with(".zip") {
        let file = fs::File::open(archive_path)?;
        let mut archive = ZipArchive::new(file)?;
        archive.extract(root)?;
        Ok(())
    } else if name.ends_with(".tar.gz") {
        let file = fs::File::open(archive_path)?;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);
        archive.unpack(root)?;
        Ok(())
    } else {
        Err("unsupported archive format".into())
    }
}

fn package_version(root: &Path) -> Option<String> {
    let manifest = fs::read_to_string(root.join("Cargo.toml")).ok()?;
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

fn platform_package_suffixes() -> &'static [&'static str] {
    match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => &["x86_64.linux"],
        ("macos", "aarch64") => &["arm64.macos"],
        ("macos", "x86_64") => &["x86_64.macos"],
        ("windows", "x86_64") => &["x64.windows", "x64.vc14.windows"],
        _ => &[],
    }
}
