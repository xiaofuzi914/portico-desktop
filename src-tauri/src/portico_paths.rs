//! User-facing Portico home layout under `~/.portico`.
//!
//! Layout:
//! ```text
//! ~/.portico/
//!   plugins/
//!     installed/   # active packages keyed by plugin UUID
//!     available/   # drop-in package folders (plugin.json) ready for one-click install
//! ```
//!
//! When running desktop-e2e, the host passes an isolated home under the e2e data dir.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const HOME_DIR_NAME: &str = ".portico";
const PLUGINS_DIR: &str = "plugins";
const INSTALLED_DIR: &str = "installed";
const AVAILABLE_DIR: &str = "available";

/// Resolved user Portico directories.
#[derive(Debug, Clone, Serialize)]
pub struct PorticoUserDirs {
    /// `~/.portico` (or an isolated e2e home).
    pub home: PathBuf,
    /// `…/plugins`.
    pub plugins: PathBuf,
    /// Active install root: `…/plugins/installed`.
    pub plugins_installed: PathBuf,
    /// Drop-in packages: `…/plugins/available`.
    pub plugins_available: PathBuf,
}

/// Resolve the default Portico home (`$HOME/.portico`).
///
/// # Errors
///
/// Returns an error when the user home directory cannot be resolved.
pub fn default_portico_home() -> Result<PathBuf, String> {
    let user_home =
        dirs::home_dir().ok_or_else(|| "failed to resolve user home directory".to_owned())?;
    Ok(user_home.join(HOME_DIR_NAME))
}

/// Ensure the Portico directory tree exists under `home`.
///
/// # Errors
///
/// Returns an error when directories cannot be created.
pub fn ensure_portico_user_dirs_at(home: PathBuf) -> Result<PorticoUserDirs, String> {
    let plugins = home.join(PLUGINS_DIR);
    let plugins_installed = plugins.join(INSTALLED_DIR);
    let plugins_available = plugins.join(AVAILABLE_DIR);
    for path in [&home, &plugins, &plugins_installed, &plugins_available] {
        fs::create_dir_all(path)
            .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    }
    let readme = plugins_available.join("README.txt");
    if !readme.exists() {
        let _ = fs::write(
            readme,
            "Drop plugin package folders here (each must contain plugin.json).\n\
             Then open Portico → Capabilities → Plugins and click Install on the package.\n\
             Installed packages live in ../installed/.\n",
        );
    }
    Ok(PorticoUserDirs {
        home,
        plugins,
        plugins_installed,
        plugins_available,
    })
}

/// Ensure the default `~/.portico` layout exists.
pub fn ensure_default_portico_user_dirs() -> Result<PorticoUserDirs, String> {
    ensure_portico_user_dirs_at(default_portico_home()?)
}

/// Scan `plugins/available` for directories that contain a `plugin.json`.
pub fn list_available_packages(available_root: &Path) -> Result<Vec<AvailablePluginPackage>, String> {
    if !available_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut packages = Vec::new();
    let entries = fs::read_dir(available_root)
        .map_err(|error| format!("failed to read available plugins: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to read available plugin entry: {error}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let manifest_path = path.join("plugin.json");
        if !manifest_path.is_file() {
            continue;
        }
        let bytes = fs::read(&manifest_path)
            .map_err(|error| format!("failed to read {}: {error}", manifest_path.display()))?;
        let manifest: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid plugin.json in {name}: {error}"))?;
        packages.push(AvailablePluginPackage {
            folder_name: name,
            path: path.to_string_lossy().into_owned(),
            display_name: manifest
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_owned(),
            plugin_name: manifest
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned(),
            version: manifest
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned(),
            description: manifest
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned(),
            plugin_id: manifest
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned(),
        });
    }
    packages.sort_by(|a, b| a.folder_name.cmp(&b.folder_name));
    Ok(packages)
}

/// A package folder sitting in `…/plugins/available`.
#[derive(Debug, Clone, Serialize)]
pub struct AvailablePluginPackage {
    pub folder_name: String,
    pub path: String,
    pub display_name: String,
    pub plugin_name: String,
    pub version: String,
    pub description: String,
    pub plugin_id: String,
}

/// Bundled catalog package folder names shipped with the Portico monorepo.
pub const BUNDLED_CATALOG_PACKAGE_NAMES: &[&str] = &[
    "markdown-viewer-provider",
    "markdown-rich-provider",
];

/// Resolve the monorepo / resource directory that contains catalog packages.
pub fn bundled_plugins_root() -> Option<PathBuf> {
    // Dev / monorepo: src-tauri/../examples/plugins
    let from_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/plugins");
    if from_manifest.is_dir() {
        return Some(from_manifest);
    }
    // Optional: next to the running binary (future resource bundle).
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join("plugins");
        if candidate.is_dir() {
            return Some(candidate);
        }
        let candidate = dir.join("../Resources/plugins");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

/// Copy a single catalog package into `plugins/available/{name}`.
///
/// Always refreshes from the monorepo/resource source when present so package
/// fixes (performance, protocol) land without manual copy.
pub fn seed_catalog_package_to_available(
    available_root: &Path,
    package_name: &str,
) -> Result<PathBuf, String> {
    if !BUNDLED_CATALOG_PACKAGE_NAMES.contains(&package_name) {
        return Err(format!("unknown catalog package: {package_name}"));
    }
    let dest = available_root.join(package_name);
    let Some(root) = bundled_plugins_root() else {
        if dest.join("plugin.json").is_file() {
            return Ok(dest);
        }
        return Err(
            "bundled plugin packages are not available in this build; place the package under ~/.portico/plugins/available first"
                .to_owned(),
        );
    };
    let source = root.join(package_name);
    if !source.join("plugin.json").is_file() {
        if dest.join("plugin.json").is_file() {
            return Ok(dest);
        }
        return Err(format!(
            "catalog package source missing: {}",
            source.display()
        ));
    }
    if dest.exists() {
        fs::remove_dir_all(&dest)
            .map_err(|error| format!("failed to clear stale available package: {error}"))?;
    }
    copy_dir_all(&source, &dest)
        .map_err(|error| format!("failed to seed catalog package: {error}"))?;
    Ok(dest)
}

/// Seed all known catalog packages into `available/`.
pub fn seed_all_catalog_packages(available_root: &Path) -> Result<usize, String> {
    let mut seeded = 0usize;
    for name in BUNDLED_CATALOG_PACKAGE_NAMES {
        match seed_catalog_package_to_available(available_root, name) {
            Ok(_) => seeded = seeded.saturating_add(1),
            Err(_) => {
                // Missing source in production is non-fatal for other packages.
            }
        }
    }
    Ok(seeded)
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn ensure_creates_layout_under_given_home() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join(".portico");
        let dirs = ensure_portico_user_dirs_at(home).expect("dirs");
        assert!(dirs.plugins_installed.is_dir());
        assert!(dirs.plugins_available.is_dir());
        assert!(dirs.plugins_available.join("README.txt").is_file());
    }

    #[test]
    fn lists_available_packages_with_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let available = temp.path().join("available");
        let pkg = available.join("demo-plugin");
        fs::create_dir_all(&pkg).unwrap();
        let mut file = fs::File::create(pkg.join("plugin.json")).unwrap();
        write!(
            file,
            r#"{{"id":"11111111-1111-1111-1111-111111111111","name":"demo","version":"1.0.0","display_name":"Demo","description":"d","skills":[],"tools":[],"entrypoint":"index.html","capabilities":[],"install_path":null,"permissions":{{"network":[],"filesystem":"none"}},"enabled":true,"installed_at":"2026-01-01T00:00:00Z"}}"#
        )
        .unwrap();
        let listed = list_available_packages(&available).expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].folder_name, "demo-plugin");
        assert_eq!(listed[0].display_name, "Demo");
    }
}
