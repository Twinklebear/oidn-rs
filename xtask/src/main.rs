use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

type DynError = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, DynError>;

const HELP: &str = "\
oidn-rs development tasks

Usage:
  cargo run -p xtask -- build-examples [cargo-options...]
  cargo run -p xtask -- build-test [cargo-options...]
  cargo run -p xtask -- generate-sys-bindings [oidn.h] [src/sys.rs]

Aliases:
  build-examples-linux-mac -> build-examples
  build-test-mac           -> build-test
  build-test-windows       -> build-test
";

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
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
        other => return Err(format!("unknown xtask command `{other}`\n\n{HELP}").into()),
    }

    Ok(())
}

fn build_examples(root: &Path, extra_args: &[OsString]) -> Result<()> {
    let envs = oidn_environment(root)?;
    run_cargo(root, &["build", "--examples"], extra_args, &envs)
}

fn build_test(root: &Path, extra_args: &[OsString]) -> Result<()> {
    let envs = oidn_environment(root)?;
    println!("Building oidn-rs");
    run_cargo(root, &["build"], extra_args, &envs)?;
    println!("Running oidn-rs tests");
    run_cargo(root, &["test"], extra_args, &envs)?;
    println!("Running oidn-rs example tests");
    run_cargo(root, &["test", "--examples"], extra_args, &envs)
}

fn generate_sys_bindings(root: &Path, args: &[OsString]) -> Result<()> {
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

    let bindgen_args = bindgen_args(&header, &output);
    let envs = bindgen_environment();
    run_command(root, "bindgen", &bindgen_args, &envs)
}

fn bindgen_args(header: &Path, output: &Path) -> Vec<OsString> {
    vec![
        header.as_os_str().to_os_string(),
        "-o".into(),
        output.as_os_str().to_os_string(),
        "--no-doc-comments".into(),
        "--distrust-clang-mangling".into(),
        "--allowlist-function".into(),
        "oidn.*".into(),
        "--allowlist-type".into(),
        "OIDN.*".into(),
        "--".into(),
        "-x".into(),
        "c++".into(),
        "-std=c++11".into(),
    ]
}

fn run_cargo(
    root: &Path,
    args: &[&str],
    extra_args: &[OsString],
    envs: &[(String, OsString)],
) -> Result<()> {
    let mut cargo_args = args.iter().map(OsString::from).collect::<Vec<_>>();
    cargo_args.extend(extra_args.iter().cloned());
    run_command(root, "cargo", &cargo_args, envs)
}

fn run_command(
    root: &Path,
    program: &str,
    args: &[OsString],
    envs: &[(String, OsString)],
) -> Result<()> {
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

fn oidn_environment(root: &Path) -> Result<Vec<(String, OsString)>> {
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

fn find_oidn_header(root: &Path) -> Result<PathBuf> {
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

fn oidn_package_dirs(root: &Path, version: &str) -> Vec<PathBuf> {
    platform_package_suffixes()
        .iter()
        .map(|suffix| root.join(format!("oidn-{version}.{suffix}")))
        .collect()
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

fn oidn_dir(root: &Path) -> Option<PathBuf> {
    if let Some(dir) = non_empty_env_path("OIDN_DIR") {
        return Some(dir);
    }

    let version = env::var("OIDN_VERSION")
        .ok()
        .filter(|version| !version.is_empty())
        .or_else(|| package_version(root))?;

    oidn_package_dirs(root, &version)
        .into_iter()
        .find(|dir| dir.is_dir())
}

fn non_empty_env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
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

fn runtime_library_path(oidn_dir: &Path) -> Option<(&'static str, PathBuf)> {
    match env::consts::OS {
        "linux" => Some(("LD_LIBRARY_PATH", oidn_dir.join("lib"))),
        "macos" => Some(("DYLD_LIBRARY_PATH", oidn_dir.join("lib"))),
        "windows" => Some(("PATH", oidn_dir.join("bin"))),
        _ => None,
    }
}

fn appended_path(variable: &str, path: &Path) -> Result<OsString> {
    let mut paths = env::var_os(variable)
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    paths.push(path.to_path_buf());
    env::join_paths(paths).map_err(|error| error.into())
}

fn bindgen_environment() -> Vec<(String, OsString)> {
    if env::var_os("LIBCLANG_PATH").is_some() {
        return Vec::new();
    }

    let Some(path) = detect_libclang_dir() else {
        return Vec::new();
    };

    println!("Using LIBCLANG_PATH={}", path.display());
    vec![("LIBCLANG_PATH".to_string(), path.into_os_string())]
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

fn default_llvm_dirs() -> Vec<PathBuf> {
    if env::consts::OS == "windows" {
        vec![
            PathBuf::from(r"C:\Program Files\LLVM\bin"),
            PathBuf::from(r"C:\Program Files\LLVM\lib"),
            PathBuf::from(r"C:\Program Files (x86)\LLVM\bin"),
            PathBuf::from(r"C:\Program Files (x86)\LLVM\lib"),
        ]
    } else {
        vec![
            PathBuf::from("/usr/lib"),
            PathBuf::from("/usr/local/lib"),
            PathBuf::from("/opt/homebrew/opt/llvm/lib"),
            PathBuf::from("/usr/local/opt/llvm/lib"),
        ]
    }
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

fn workspace_root() -> Result<PathBuf> {
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
