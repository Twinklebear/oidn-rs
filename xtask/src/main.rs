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
