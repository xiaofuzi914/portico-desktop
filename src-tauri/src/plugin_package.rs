//! Safe installation of user-selected plugin package directories.

use app_models::PluginManifest;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

const MANIFEST_FILE: &str = "plugin.json";
const MAX_FILES: usize = 10_000;
const MAX_PACKAGE_BYTES: u64 = 250 * 1024 * 1024;

/// Copy a validated plugin directory into the application's plugin store.
///
/// The source must be a real directory (not a symlink) containing a valid
/// `plugin.json`. Symlinks and special files are rejected throughout the tree.
pub fn install_directory(
    source: &Path,
    plugins_root: &Path,
) -> Result<(PluginManifest, PathBuf), String> {
    let source = canonical_real_directory(source)?;
    let mut manifest = read_manifest(&source)?;
    validate_package_name(&manifest.name)?;
    validate_entrypoint(&source, manifest.entrypoint.as_deref())?;

    fs::create_dir_all(plugins_root)
        .map_err(|error| format!("failed to create plugin store: {error}"))?;
    reject_symlink(plugins_root, "plugin store")?;

    let destination = plugins_root.join(manifest.id.0.to_string());
    let staging = plugins_root.join(format!(".install-{}", manifest.id.0));
    remove_real_directory_if_present(&staging)?;
    // Allow reinstall/replace: clear any previous package at this id.
    remove_real_directory_if_present(&destination)?;

    if let Err(error) = copy_tree(&source, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    if let Some(entrypoint) = manifest.entrypoint.as_deref() {
        harden_provider_entrypoint(&staging.join(entrypoint), &manifest)?;
    }
    if let Err(error) = fs::rename(&staging, &destination) {
        return Err(format!("failed to activate plugin package: {error}"));
    }
    manifest.install_path = Some(destination.to_string_lossy().into_owned());
    Ok((manifest, destination))
}

/// Remove an installed plugin directory (and leftover install/uninstall staging dirs).
pub fn remove_installed_package(plugins_root: &Path, plugin_id: &str) -> Result<(), String> {
    let destination = plugins_root.join(plugin_id);
    let install_staging = plugins_root.join(format!(".install-{plugin_id}"));
    let uninstall_staging = plugins_root.join(format!(".uninstall-{plugin_id}"));
    remove_real_directory_if_present(&destination)?;
    remove_real_directory_if_present(&install_staging)?;
    remove_real_directory_if_present(&uninstall_staging)?;
    Ok(())
}

/// Remove a path if it exists (for cleanup of recorded install_path values).
pub fn remove_path_if_present(path: &Path) -> Result<(), String> {
    remove_real_directory_if_present(path)
}

fn harden_provider_entrypoint(path: &Path, manifest: &PluginManifest) -> Result<(), String> {
    if manifest.capabilities.is_empty() {
        return Ok(());
    }
    let connect_src = build_connect_src(&manifest.permissions.network)?;
    let html = fs::read_to_string(path)
        .map_err(|error| format!("plugin UI entrypoint must be UTF-8 HTML: {error}"))?;
    // wasm-unsafe-eval + worker-src: modern diagram engines (Mermaid, etc.).
    // Network only when the package declares explicit hosts in permissions.network.
    let policy = format!(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'self'; connect-src {connect_src}; img-src 'self' data: blob:; media-src 'none'; object-src 'none'; frame-src 'none'; form-action 'none'; base-uri 'none'; navigate-to 'none'; style-src 'self' 'unsafe-inline'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self' blob:\">"
    );
    let Some(index) = html.to_lowercase().find("<head>") else {
        return Err("plugin UI entrypoint must contain an HTML head element".to_owned());
    };
    let insertion = index + "<head>".len();
    let hardened = format!("{}{}{}", &html[..insertion], policy, &html[insertion..]);
    fs::write(path, hardened)
        .map_err(|error| format!("failed to harden plugin UI entrypoint: {error}"))
}

/// Build CSP `connect-src` from declared network hosts (https only).
fn build_connect_src(network: &[String]) -> Result<String, String> {
    if network.is_empty() {
        return Ok("'none'".to_owned());
    }
    let mut parts = Vec::with_capacity(network.len());
    for host in network {
        let trimmed = host.trim();
        if trimmed.is_empty() || trimmed.contains(' ') || trimmed.contains(';') {
            return Err(format!("invalid plugin network host: {host}"));
        }
        if !(trimmed.starts_with("https://")
            || trimmed.starts_with("http://localhost")
            || trimmed.starts_with("http://127.0.0.1"))
        {
            return Err(format!(
                "plugin network host must be https:// or local http: {host}"
            ));
        }
        parts.push(trimmed.to_owned());
    }
    Ok(parts.join(" "))
}

fn canonical_real_directory(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("plugin package path must be absolute".to_owned());
    }
    reject_symlink(path, "plugin package")?;
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("failed to resolve plugin package: {error}"))?;
    if !canonical.is_dir() {
        return Err("plugin package must be a directory".to_owned());
    }
    Ok(canonical)
}

fn reject_symlink(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {label}: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("{label} cannot be a symbolic link"));
    }
    Ok(())
}

/// Public helper for install orchestration (preview id before copy).
pub fn read_manifest_public(source: &Path) -> Result<PluginManifest, String> {
    read_manifest(source)
}

fn read_manifest(source: &Path) -> Result<PluginManifest, String> {
    let path = source.join(MANIFEST_FILE);
    reject_symlink(&path, "plugin manifest")?;
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("plugin package is missing {MANIFEST_FILE}: {error}"))?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err("plugin manifest must be a regular file no larger than 1 MiB".to_owned());
    }
    let bytes =
        fs::read(&path).map_err(|error| format!("failed to read plugin manifest: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid plugin manifest: {error}"))
}

fn validate_package_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 128
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(
            "plugin name must contain only ASCII letters, digits, '.', '-' or '_'".to_owned(),
        );
    }
    Ok(())
}

fn validate_entrypoint(source: &Path, entrypoint: Option<&str>) -> Result<(), String> {
    let Some(entrypoint) = entrypoint else {
        return Ok(());
    };
    let relative = Path::new(entrypoint);
    if entrypoint.is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("plugin entrypoint must be a normalized relative path".to_owned());
    }
    let target = source.join(relative);
    reject_symlink(&target, "plugin entrypoint")?;
    if !target.is_file() {
        return Err("plugin entrypoint must reference a regular file in the package".to_owned());
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir(destination)
        .map_err(|error| format!("failed to create plugin staging directory: {error}"))?;
    let mut file_count = 0usize;
    let mut total_bytes = 0u64;
    copy_directory(source, destination, &mut file_count, &mut total_bytes)
}

fn copy_directory(
    source: &Path,
    destination: &Path,
    file_count: &mut usize,
    total_bytes: &mut u64,
) -> Result<(), String> {
    for entry in fs::read_dir(source).map_err(|error| format!("failed to read package: {error}"))? {
        let entry = entry.map_err(|error| format!("failed to inspect package entry: {error}"))?;
        let name = entry.file_name();
        if Path::new(&name).components().any(|part| !matches!(part, Component::Normal(_))) {
            return Err("plugin package contains an invalid path".to_owned());
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| format!("failed to inspect package entry: {error}"))?;
        let target = destination.join(name);
        if metadata.file_type().is_symlink() {
            return Err("plugin packages cannot contain symbolic links".to_owned());
        }
        if metadata.is_dir() {
            fs::create_dir(&target)
                .map_err(|error| format!("failed to create plugin directory: {error}"))?;
            copy_directory(&entry.path(), &target, file_count, total_bytes)?;
        } else if metadata.is_file() {
            *file_count += 1;
            *total_bytes = total_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| "plugin package size overflow".to_owned())?;
            if *file_count > MAX_FILES || *total_bytes > MAX_PACKAGE_BYTES {
                return Err("plugin package exceeds the installation size limit".to_owned());
            }
            copy_regular_file(&entry.path(), &target, metadata.len())?;
        } else {
            return Err("plugin packages can contain only files and directories".to_owned());
        }
    }
    Ok(())
}

fn copy_regular_file(source: &Path, target: &Path, expected_len: u64) -> Result<(), String> {
    let input =
        fs::File::open(source).map_err(|error| format!("failed to open plugin file: {error}"))?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|error| format!("failed to create plugin file: {error}"))?;
    let copied = io::copy(&mut input.take(expected_len + 1), &mut output)
        .map_err(|error| format!("failed to copy plugin file: {error}"))?;
    output
        .flush()
        .map_err(|error| format!("failed to flush plugin file: {error}"))?;
    if copied != expected_len {
        return Err("plugin package changed while it was being installed".to_owned());
    }
    Ok(())
}

fn remove_real_directory_if_present(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    reject_symlink(path, "plugin staging directory")?;
    fs::remove_dir_all(path)
        .map_err(|error| format!("failed to clean plugin staging directory: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_models::{PluginCapability, PluginId, PluginPermissions};
    use chrono::Utc;
    use tempfile::TempDir;

    fn package(root: &Path) -> PluginManifest {
        let manifest = PluginManifest {
            id: PluginId::new(),
            name: "markdown-provider".to_owned(),
            version: "1.0.0".to_owned(),
            display_name: "Markdown Provider".to_owned(),
            description: "test".to_owned(),
            skills: vec![],
            tools: vec![],
            entrypoint: Some("dist/index.html".to_owned()),
            capabilities: vec![PluginCapability::MarkdownPreview],
            install_path: None,
            permissions: PluginPermissions {
                network: vec![],
                filesystem: "none".to_owned(),
            },
            enabled: true,
            installed_at: Utc::now(),
        };
        fs::write(
            root.join(MANIFEST_FILE),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        fs::create_dir(root.join("dist")).unwrap();
        fs::write(
            root.join("dist/index.html"),
            "<!doctype html><html><head></head><body>hello</body></html>",
        )
        .unwrap();
        manifest
    }

    #[test]
    fn installs_a_valid_directory_and_allows_replacement() {
        let source = TempDir::new().unwrap();
        let store = TempDir::new().unwrap();
        let manifest = package(source.path());
        let (_, destination) = install_directory(source.path(), store.path()).unwrap();
        let installed_html = fs::read_to_string(destination.join("dist/index.html")).unwrap();
        assert!(installed_html.contains("hello"));
        assert!(installed_html.contains("connect-src 'none'"));

        // Reinstall must replace files rather than no-op or error.
        fs::write(
            source.path().join("dist/index.html"),
            "<!doctype html><html><head></head><body>updated</body></html>",
        )
        .unwrap();
        let (_, destination2) = install_directory(source.path(), store.path()).unwrap();
        assert_eq!(destination, destination2);
        let replaced = fs::read_to_string(destination2.join("dist/index.html")).unwrap();
        assert!(replaced.contains("updated"));
        assert!(destination.ends_with(manifest.id.0.to_string()));
    }

    #[test]
    fn remove_installed_package_deletes_directory() {
        let source = TempDir::new().unwrap();
        let store = TempDir::new().unwrap();
        let manifest = package(source.path());
        let (_, destination) = install_directory(source.path(), store.path()).unwrap();
        assert!(destination.is_dir());
        remove_installed_package(store.path(), &manifest.id.0.to_string()).unwrap();
        assert!(!destination.exists());
    }

    #[test]
    fn rejects_relative_paths_and_missing_manifests() {
        let source = TempDir::new().unwrap();
        let store = TempDir::new().unwrap();
        assert!(
            install_directory(Path::new("relative"), store.path())
                .unwrap_err()
                .contains("absolute")
        );
        assert!(install_directory(source.path(), store.path()).unwrap_err().contains("manifest"));
    }

    #[test]
    fn rejects_an_entrypoint_outside_the_package() {
        let source = TempDir::new().unwrap();
        let store = TempDir::new().unwrap();
        let mut manifest = package(source.path());
        manifest.entrypoint = Some("../index.html".to_owned());
        fs::write(
            source.path().join(MANIFEST_FILE),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(
            install_directory(source.path(), store.path())
                .unwrap_err()
                .contains("entrypoint")
        );
    }

    #[test]
    fn rejects_a_provider_entrypoint_without_an_html_head() {
        let source = TempDir::new().unwrap();
        let store = TempDir::new().unwrap();
        package(source.path());
        fs::write(source.path().join("dist/index.html"), "hello").unwrap();
        assert!(
            install_directory(source.path(), store.path())
                .unwrap_err()
                .contains("head element")
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_anywhere_in_the_package() {
        use std::os::unix::fs::symlink;
        let source = TempDir::new().unwrap();
        let store = TempDir::new().unwrap();
        package(source.path());
        symlink("dist/index.html", source.path().join("alias.html")).unwrap();
        assert!(
            install_directory(source.path(), store.path())
                .unwrap_err()
                .contains("symbolic links")
        );
        assert!(fs::read_dir(store.path()).unwrap().next().is_none());
    }
}
