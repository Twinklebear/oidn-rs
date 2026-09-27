#[cfg(not(feature = "bundled"))]
compile_error!(
    "bundled feature is required because of the shared code in `shared/helper.rs` that is used by both build.rs and xtask/src/main.rs. Set OIDN_DIR or OIDN_BUNDLED_DIR to point at a local OpenImageDenoise installation to build against it."
);

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

type DynError = Box<dyn std::error::Error>;
type DynResult<T> = std::result::Result<T, DynError>;

include!("../../shared/helper.rs");

const HELP: &str = "\
oidn-rs development tasks

Usage:
  cargo run -p xtask -- build-examples [cargo-options...]
  cargo run -p xtask -- build-test [cargo-options...]
  cargo run -p xtask -- generate-sys-bindings [oidn.h] [src/sys.rs]
  cargo run -p xtask -- download-oidn-package
  cargo run -p xtask -- check-coverage
  cargo run -p xtask -- update-oidn <version>

update-oidn downloads the official packages of an Open Image Denoise release,
records their hashes, bumps the version, and regenerates src/sys.rs from the
release's oidn.h. The committed bindings are generated on Windows; other
hosts may emit different integer types for enums.

Aliases:
  build-examples-linux-mac -> build-examples
  build-test-mac           -> build-test
  build-test-windows       -> build-test
  download-oidn -> download-oidn-package
";

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> DynResult<()> {
    let root = workspace_root()?;
    let mut args = env::args_os();
    let _program = args.next();

    let Some(command) = args.next() else {
        print!("{HELP}");
        return Ok(());
    };

    let command = command
        .to_str()
        .ok_or("xtask command must be valid UTF-8")?;
    let args = args.collect::<Vec<_>>();

    match command {
        "-h" | "--help" | "help" => print!("{HELP}"),
        "build-examples" | "build-examples-linux-mac" => build_examples(&root, &args)?,
        "build-test" | "build-test-mac" | "build-test-windows" => build_test(&root, &args)?,
        "generate-sys-bindings" => generate_sys_bindings(&root, &args)?,
        "download-oidn-package" | "download-oidn" => download_oidn_package(&root, &args)?,
        "check-coverage" => check_coverage(&root, &args)?,
        "update-oidn" => update_oidn(&root, &args)?,
        other => return Err(format!("unknown xtask command `{other}`\n\n{HELP}").into()),
    }

    Ok(())
}

fn build_examples(root: &Path, extra_args: &[OsString]) -> DynResult<()> {
    let envs = oidn_environment(root)?;
    run_cargo(root, &["build", "--examples"], extra_args, &envs)
}

fn check_coverage(root: &Path, args: &[OsString]) -> DynResult<()> {
    if !args.is_empty() {
        return Err("usage: cargo run -p xtask -- check-coverage".into());
    }

    let envs = oidn_environment(root)?;

    run_cargo(root, &["llvm-cov", "clean", "--workspace"], &[], &envs)?;
    run_cargo(
        root,
        &[
            "llvm-cov",
            "test",
            "--workspace",
            "--all-features",
            "--all-targets",
            "--no-report",
        ],
        &[],
        &envs,
    )?;

    // Examples that need neither command line arguments nor input images.
    for example in ["buffer", "async_buffers"] {
        run_cargo(
            root,
            &["llvm-cov", "run", "--example", example, "--no-report"],
            &[],
            &envs,
        )?;
    }

    run_cargo(root, &["llvm-cov", "report", "--html"], &[], &envs)?;
    run_cargo(root, &["llvm-cov", "report"], &[], &envs)?;

    println!(
        "HTML report: {}",
        root.join("target/llvm-cov/html/index.html").display()
    );

    Ok(())
}

fn build_test(root: &Path, extra_args: &[OsString]) -> DynResult<()> {
    let envs = oidn_environment(root)?;
    println!("Building oidn-rs");
    run_cargo(root, &["build"], extra_args, &envs)?;
    println!("Running oidn-rs tests");
    run_cargo(root, &["test"], extra_args, &envs)?;
    println!("Running oidn-rs example tests");
    run_cargo(root, &["test", "--examples"], extra_args, &envs)
}

fn generate_sys_bindings(root: &Path, args: &[OsString]) -> DynResult<()> {
    if args.len() > 2 {
        return Err(
            "usage: cargo run -p xtask -- generate-sys-bindings [oidn.h] [src/sys.rs]".into(),
        );
    }

    let header = match args.first() {
        Some(path) => workspace_path(root, path),
        None => find_oidn_header(root)?,
    };
    let output = match args.get(1) {
        Some(path) => workspace_path(root, path),
        None => root.join("src").join("sys.rs"),
    };

    println!(
        "Generating bindings from {} to {}",
        header.display(),
        output.display()
    );

    generate_bindings(&header, &output)
}

fn download_oidn_package(root: &Path, _args: &[OsString]) -> DynResult<()> {
    let package_dir = download_and_extract_oidn(root)?;
    println!("OIDN package available at {}", package_dir.display());
    Ok(())
}

/// The official binary packages the `bundled` feature downloads, as package
/// suffix and archive extension. Each has its hash in
/// `oidn_hashes/<package suffix>.sha256`.
const OIDN_PACKAGES: &[(&str, &str)] = &[
    ("x86_64.linux", "tar.gz"),
    ("x86_64.macos", "tar.gz"),
    ("arm64.macos", "tar.gz"),
    ("x64.windows", "zip"),
];

fn update_oidn(root: &Path, args: &[OsString]) -> DynResult<()> {
    let [version] = args else {
        return Err("usage: cargo run -p xtask -- update-oidn <version>".into());
    };
    let version = version.to_str().ok_or("version must be valid UTF-8")?;
    let is_version = version.split('.').count() == 3
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
    if !is_version {
        return Err(format!("`{version}` is not a version like 2.5.1").into());
    }

    let old_version =
        package_version(root).ok_or("could not read the package version from Cargo.toml")?;
    if old_version == version {
        println!("Already at Open Image Denoise {version}");
        return Ok(());
    }

    // Download everything before changing any file, so a release that is
    // missing a package leaves the tree untouched.
    let mut hashes = Vec::new();
    for (package, extension) in OIDN_PACKAGES {
        let archive_name = format!("oidn-{version}.{package}.{extension}");
        let archive_path = root.join(&archive_name);
        download_archive(
            &archive_path,
            &format!(
                "https://github.com/OpenImageDenoise/oidn/releases/download/v{version}/{archive_name}"
            ),
        )?;
        let hash = sha256_hex(&archive_path)?;
        println!("{archive_name}: {hash}");
        hashes.push((package, hash));
    }

    for (package, hash) in hashes {
        fs::write(
            root.join("oidn_hashes").join(format!("{package}.sha256")),
            hash,
        )?;
    }

    replace_in_file(
        &root.join("Cargo.toml"),
        &format!("version = \"{old_version}\""),
        &format!("version = \"{version}\""),
        Some(1),
    )?;
    replace_in_file(&root.join("README.md"), &old_version, version, None)?;

    let host_package = platform_package_suffixes()[0];
    let package_dir = root.join(format!("oidn-{version}.{host_package}"));
    if !package_dir.is_dir() {
        let (_, extension) = OIDN_PACKAGES
            .iter()
            .find(|(package, _)| *package == host_package)
            .ok_or("no official package for this host")?;
        extract_archive(
            root,
            &root.join(format!("oidn-{version}.{host_package}.{extension}")),
        )?;
    }
    generate_bindings(
        &package_dir
            .join("include")
            .join("OpenImageDenoise")
            .join("oidn.h"),
        &root.join("src").join("sys.rs"),
    )?;

    println!("Updated Open Image Denoise from {old_version} to {version}");
    Ok(())
}

/// Replaces `from` with `to` in a file, at most `limit` times, failing if
/// `from` does not occur at all.
fn replace_in_file(path: &Path, from: &str, to: &str, limit: Option<usize>) -> DynResult<()> {
    let contents = fs::read_to_string(path)?;
    if !contents.contains(from) {
        return Err(format!("`{from}` not found in {}", path.display()).into());
    }

    let contents = match limit {
        Some(limit) => contents.replacen(from, to, limit),
        None => contents.replace(from, to),
    };
    fs::write(path, contents)?;
    Ok(())
}

fn run_cargo(
    root: &Path,
    args: &[&str],
    extra_args: &[OsString],
    envs: &[(String, OsString)],
) -> DynResult<()> {
    let mut cargo_args = args.iter().map(OsString::from).collect::<Vec<_>>();
    cargo_args.extend(extra_args.iter().cloned());
    run_command(root, "cargo", &cargo_args, envs)
}

fn run_command(
    root: &Path,
    program: &str,
    args: &[OsString],
    envs: &[(String, OsString)],
) -> DynResult<()> {
    println!("running: {}", format_command(program, args));

    let mut command = ProcessCommand::new(program);
    command.current_dir(root).args(args);
    for (key, value) in envs {
        command.env(key, value);
    }

    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "`{}` failed with status {}",
            format_command(program, args),
            status
        )
        .into())
    }
}

/// Returns the environment variables needed to build and run against the
/// configured OIDN installation, or nothing if OIDN_DIR is not set and no
/// local package was found.
fn oidn_environment(root: &Path) -> DynResult<Vec<(String, OsString)>> {
    let Some(oidn_dir) = oidn_dir(root) else {
        return Ok(Vec::new());
    };

    println!("Using OIDN_DIR={}", oidn_dir.display());
    let mut envs = vec![("OIDN_DIR".to_string(), oidn_dir.as_os_str().to_os_string())];

    let Some((variable, path)) = runtime_library_path(&oidn_dir) else {
        return Ok(envs);
    };

    if path.is_dir() {
        let value = appended_path(variable, &path)?;
        envs.push((variable.to_string(), value));
    }

    Ok(envs)
}

fn find_oidn_header(root: &Path) -> DynResult<PathBuf> {
    header_candidates(root)
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            "could not find oidn.h; set OIDN_HEADER/OIDN_DIR/OIDN_BUNDLED_DIR or pass the header path explicitly"
                .into()
        })
}

fn header_candidates(root: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(header) = non_empty_env_path("OIDN_HEADER") {
        candidates.push(header);
    }

    candidates.extend(
        ["OIDN_DIR", "OIDN_BUNDLED_DIR"]
            .into_iter()
            .filter_map(non_empty_env_path)
            .flat_map(header_candidates_for_oidn_dir),
    );

    candidates.extend(
        package_version(root)
            .into_iter()
            .flat_map(|version| oidn_package_dirs(root, &version))
            .flat_map(header_candidates_for_oidn_dir),
    );

    candidates.extend(find_target_oidn_headers(&root.join("target")));
    candidates
}

fn header_candidates_for_oidn_dir(dir: PathBuf) -> Vec<PathBuf> {
    vec![
        dir.join("include").join("OpenImageDenoise").join("oidn.h"),
        dir.join("include").join("oidn.h"),
    ]
}

fn find_target_oidn_headers(target_dir: &Path) -> Vec<PathBuf> {
    let mut headers = Vec::new();
    collect_target_oidn_headers(target_dir, &mut headers);
    headers
}

fn collect_target_oidn_headers(dir: &Path, headers: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_target_oidn_headers(&path, headers);
        } else if path
            .components()
            .any(|component| component.as_os_str() == OsStr::new("OpenImageDenoise"))
            && path.file_name() == Some(OsStr::new("oidn.h"))
        {
            headers.push(path);
        }
    }
}

fn runtime_library_path(oidn_dir: &Path) -> Option<(&'static str, PathBuf)> {
    match env::consts::OS {
        "linux" => Some(("LD_LIBRARY_PATH", oidn_dir.join("lib"))),
        "macos" => Some(("DYLD_LIBRARY_PATH", oidn_dir.join("lib"))),
        "windows" => Some(("PATH", oidn_dir.join("bin"))),
        _ => None,
    }
}

fn appended_path(variable: &str, path: &Path) -> DynResult<OsString> {
    let mut paths = env::var_os(variable)
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    paths.push(path.to_path_buf());
    env::join_paths(paths).map_err(|error| error.into())
}

fn detect_libclang_dir() -> Option<PathBuf> {
    llvm_env_dirs()
        .into_iter()
        .chain(default_llvm_dirs())
        .chain(python_libclang_dirs())
        .find(|dir| contains_libclang(dir))
}

fn llvm_env_dirs() -> Vec<PathBuf> {
    ["LLVM_HOME", "LLVM_DIR"]
        .into_iter()
        .filter_map(non_empty_env_path)
        .flat_map(|dir| [dir.join("bin"), dir.join("lib"), dir])
        .collect()
}

#[cfg(target_os = "windows")]
fn default_llvm_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from(r"C:\Program Files\LLVM\bin"),
        PathBuf::from(r"C:\Program Files\LLVM\lib"),
        PathBuf::from(r"C:\Program Files (x86)\LLVM\bin"),
        PathBuf::from(r"C:\Program Files (x86)\LLVM\lib"),
    ]
}

#[cfg(target_os = "macos")]
fn default_llvm_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/lib"),
        PathBuf::from("/usr/local/lib"),
        PathBuf::from("/opt/homebrew/opt/llvm/lib"),
        PathBuf::from("/usr/local/opt/llvm/lib"),
    ]
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_llvm_dirs() -> Vec<PathBuf> {
    vec![PathBuf::from("/usr/lib"), PathBuf::from("/usr/local/lib")]
}

fn python_libclang_dirs() -> Vec<PathBuf> {
    let script = "import clang, os; print(os.path.join(os.path.dirname(os.path.realpath(clang.__file__)), 'native'))";
    ["python", "python3"]
        .into_iter()
        .filter_map(|python| python_libclang_dir(python, script))
        .collect()
}

fn python_libclang_dir(python: &str, script: &str) -> Option<PathBuf> {
    let output = ProcessCommand::new(python)
        .args(["-c", script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8(output.stdout).ok()?;
    Some(PathBuf::from(path.trim()))
}

fn contains_libclang(dir: &Path) -> bool {
    libclang_file_names()
        .iter()
        .any(|file_name| dir.join(file_name).is_file())
}

fn libclang_file_names() -> &'static [&'static str] {
    match env::consts::OS {
        "windows" => &["libclang.dll"],
        "macos" => &["libclang.dylib"],
        _ => &["libclang.so", "libclang.so.1"],
    }
}

fn workspace_root() -> DynResult<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask manifest directory has no parent".into())
}

fn workspace_path(root: &Path, path: &OsStr) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn format_command(program: &str, args: &[OsString]) -> String {
    std::iter::once(OsString::from(program))
        .chain(args.iter().cloned())
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn generate_bindings(header: &Path, output: &Path) -> DynResult<()> {
    if let Some(path) = detect_libclang_dir()
        && env::var_os("LIBCLANG_PATH").is_none()
    {
        println!("Using LIBCLANG_PATH={}", path.display());
        unsafe {
            env::set_var("LIBCLANG_PATH", path);
        }
    }

    let bindings = bindgen::Builder::default()
        .header(header.to_string_lossy())
        .clang_arg("-x")
        .clang_arg("c++")
        .clang_arg("-std=c++11")
        .generate_comments(false)
        .trust_clang_mangling(false)
        .allowlist_function("oidn.*")
        .allowlist_type("OIDN.*")
        .generate()
        .map_err(|error| format!("failed to generate bindings: {error}"))?;

    bindings
        .write_to_file(output)
        .map_err(|error| format!("failed to write bindings to {}: {error}", output.display()))?;

    Ok(())
}
