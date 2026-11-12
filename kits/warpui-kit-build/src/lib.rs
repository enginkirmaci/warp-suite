//! Build-script facts shared by every suite binary.
//!
//! `scripts/build/build.sh` and `scripts/build/build.ps1` export `WARP_SUITE_*`; a plain
//! `cargo build` has none, so this reads `version.toml` at the repository root
//! for the build counter and reports `dev` / `unknown` for what only a release
//! build can know. The version itself is the package version: the pre-commit
//! hook keeps `Cargo.toml` in step with `version.toml`, so `CARGO_PKG_VERSION`
//! is the suite version.
//!
//! On Windows it also stamps the resource block: `FILEVERSION`/`PRODUCTVERSION`
//! as `MAJOR.MINOR.PATCH.BUILD`, the product strings and the app icon. Explorer,
//! shortcuts and installers then read the same facts the About screen shows.

use std::path::{Path, PathBuf};

/// The Legal card's copyright line. `warpui-kit-core` asserts this equals
/// `about::COPYRIGHT`, so the Windows resource and the About screen cannot
/// drift into two different strings.
pub const COPYRIGHT: &str = "© 2026 Engin Kirmaci";

/// The company named in Explorer -> Properties -> Details.
pub const COMPANY: &str = "Engin Kirmaci";

/// What one binary calls itself in the Windows resource block.
pub struct AppInfo {
    pub product_name: &'static str,
    pub file_description: &'static str,
    pub original_filename: &'static str,
    pub icon: Option<&'static str>,
}

impl AppInfo {
    pub fn new(
        product_name: &'static str,
        file_description: &'static str,
        original_filename: &'static str,
    ) -> Self {
        Self {
            product_name,
            file_description,
            original_filename,
            icon: None,
        }
    }

    /// The icon path is relative to the app's manifest directory, the same way
    /// the old per-app build scripts passed it to winresource.
    pub fn with_icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }
}

struct Facts {
    build: String,
    commit: String,
    channel: String,
    released: String,
}

impl Facts {
    fn read() -> Self {
        let source = version_file();
        let build = env("WARP_SUITE_BUILD")
            .or_else(|| {
                source
                    .as_deref()
                    .and_then(read_build)
                    .map(|b| b.to_string())
            })
            .unwrap_or_else(|| "0".to_string());
        let commit = env("WARP_SUITE_COMMIT").unwrap_or_else(|| "unknown".to_string());
        let channel = env("WARP_SUITE_CHANNEL").unwrap_or_else(|| "dev".to_string());
        let released = env("WARP_SUITE_RELEASED").unwrap_or_else(|| "not released".to_string());
        if let Some(path) = &source {
            println!("cargo:rerun-if-changed={}", path.display());
        }
        Self {
            build,
            commit,
            channel,
            released,
        }
    }
}

fn env(key: &str) -> Option<String> {
    println!("cargo:rerun-if-env-changed={key}");
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

/// The nearest `version.toml` above the crate being built. The apps sit two to
/// four directories down; the walk stops before it could leave the checkout.
fn version_file() -> Option<PathBuf> {
    let mut dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR")?);
    for _ in 0..8 {
        let candidate = dir.join("version.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

fn read_build(path: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim() == "build"
        {
            return value.trim().parse().ok();
        }
    }
    None
}

/// Emit the `WARP_SUITE_*` environment for the binary being built.
///
/// Call this from a `build.rs`; it prints the `cargo:` directives itself.
pub fn emit(app: AppInfo) {
    let facts = Facts::read();
    println!("cargo:rustc-env=WARP_SUITE_BUILD={}", facts.build);
    println!("cargo:rustc-env=WARP_SUITE_COMMIT={}", facts.commit);
    println!("cargo:rustc-env=WARP_SUITE_CHANNEL={}", facts.channel);
    println!("cargo:rustc-env=WARP_SUITE_RELEASED={}", facts.released);

    #[cfg(windows)]
    windows_resources(&app, &facts);
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
fn windows_resources(app: &AppInfo, facts: &Facts) {
    let version = env!("CARGO_PKG_VERSION");
    let mut parts = version.split('.').map(|part| part.parse::<u64>());
    let major = parts.next().and_then(Result::ok).unwrap_or(0);
    let minor = parts.next().and_then(Result::ok).unwrap_or(0);
    let patch = parts.next().and_then(Result::ok).unwrap_or(0);
    let build = facts
        .build
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("WARP_SUITE_BUILD {:?} is not a number", facts.build));
    if [major, minor, patch, build]
        .into_iter()
        .any(|field| field > 0xffff)
    {
        panic!("Windows FILEVERSION fields are 16 bit: {version} build {build}");
    }
    let packed = (major << 48) | (minor << 32) | (patch << 16) | build;

    let mut resource = winresource::WindowsResource::new();
    if let Some(icon) = app.icon {
        println!("cargo:rerun-if-changed={icon}");
        resource.set_icon(icon);
    }
    resource.set_version_info(winresource::VersionInfo::FILEVERSION, packed);
    resource.set_version_info(winresource::VersionInfo::PRODUCTVERSION, packed);
    resource.set_version_info(winresource::VersionInfo::FILEOS, 0x0004_0004);
    resource.set_version_info(winresource::VersionInfo::FILETYPE, 0x0000_0001);
    resource.set("ProductName", app.product_name);
    resource.set("FileDescription", app.file_description);
    resource.set("CompanyName", COMPANY);
    resource.set("LegalCopyright", COPYRIGHT);
    resource.set("OriginalFilename", app.original_filename);
    resource.set("FileVersion", &format!("{version}.{build}"));
    resource.set("ProductVersion", &format!("{version}.{build}"));
    if let Err(error) = resource.compile() {
        panic!("could not embed the Windows resource block: {error}");
    }
}
