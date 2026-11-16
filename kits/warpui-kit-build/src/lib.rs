//! Build-script facts shared by every suite binary.
//!
//! `scripts/build/build.sh` and `scripts/build/build.ps1` export `WARP_SUITE_*`;
//! a plain `cargo build` has none, so this reports `dev` / `unknown` for what
//! only a release build can know. The version itself is the package version:
//! the pre-commit hook keeps `Cargo.toml` in step with `version.toml`, so
//! `CARGO_PKG_VERSION` is the suite version.
//!
//! On Windows it also stamps the resource block: `FILEVERSION`/`PRODUCTVERSION`
//! as `MAJOR.MINOR.PATCH.0`, the product strings and the app icon. Explorer,
//! shortcuts and installers then read the same facts the About screen shows.

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
    commit: String,
    channel: String,
    released: String,
}

impl Facts {
    fn read() -> Self {
        let commit = env("WARP_SUITE_COMMIT").unwrap_or_else(|| "unknown".to_string());
        let channel = env("WARP_SUITE_CHANNEL").unwrap_or_else(|| "dev".to_string());
        let released = env("WARP_SUITE_RELEASED").unwrap_or_else(|| "not released".to_string());
        Self {
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

/// Emit the `WARP_SUITE_*` environment for the binary being built.
///
/// Call this from a `build.rs`; it prints the `cargo:` directives itself.
pub fn emit(app: AppInfo) {
    let facts = Facts::read();
    println!("cargo:rustc-env=WARP_SUITE_COMMIT={}", facts.commit);
    println!("cargo:rustc-env=WARP_SUITE_CHANNEL={}", facts.channel);
    println!("cargo:rustc-env=WARP_SUITE_RELEASED={}", facts.released);

    #[cfg(windows)]
    windows_resources(&app);
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
fn windows_resources(app: &AppInfo) {
    // cargo sets this for the package whose build script runs, which is the
    // app; `env!` here would expand when this crate is compiled and stamp
    // warpui-kit-build's own version on every binary instead.
    let version = std::env::var("CARGO_PKG_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    let mut parts = version.split('.').map(|part| part.parse::<u64>());
    let major = parts.next().and_then(Result::ok).unwrap_or(0);
    let minor = parts.next().and_then(Result::ok).unwrap_or(0);
    let patch = parts.next().and_then(Result::ok).unwrap_or(0);
    if [major, minor, patch]
        .into_iter()
        .any(|field| field > 0xffff)
    {
        panic!("Windows FILEVERSION fields are 16 bit: {version}");
    }
    let packed = (major << 48) | (minor << 32) | (patch << 16);

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
    resource.set("FileVersion", &version);
    resource.set("ProductVersion", &version);
    if let Err(error) = resource.compile() {
        panic!("could not embed the Windows resource block: {error}");
    }
}
