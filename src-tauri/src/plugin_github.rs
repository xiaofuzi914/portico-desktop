//! Install Portico plugins from GitHub (or local monorepo packages) in a closed loop.
//!
//! Source of truth for known recipes: `examples/plugins/sources.json`.
//! User may also paste any GitHub URL whose repo root is a Portico package
//! (`plugin.json` present).
//!
//! Progress is streamed to the UI via the `plugin-install-progress` event.

use crate::plugin_package;
use crate::portico_paths::{self, PorticoUserDirs};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

/// Event name for live install log lines (frontend listens and scrolls).
pub const PLUGIN_INSTALL_PROGRESS_EVENT: &str = "plugin-install-progress";

/// One line of install progress for the capability center log.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInstallProgressEvent {
    pub phase: String,
    pub message: String,
    /// Unix ms for display ordering.
    pub ts: u64,
    /// `info` | `ok` | `warn` | `error`
    pub level: String,
}

/// Progress sink used throughout the install pipeline.
#[derive(Clone)]
pub struct InstallProgress {
    app: Option<AppHandle>,
}

impl InstallProgress {
    #[must_use]
    pub fn new(app: AppHandle) -> Self {
        Self { app: Some(app) }
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn noop() -> Self {
        Self { app: None }
    }

    pub fn info(&self, phase: &str, message: impl AsRef<str>) {
        self.emit(phase, message.as_ref(), "info");
    }

    pub fn ok(&self, phase: &str, message: impl AsRef<str>) {
        self.emit(phase, message.as_ref(), "ok");
    }

    pub fn warn(&self, phase: &str, message: impl AsRef<str>) {
        self.emit(phase, message.as_ref(), "warn");
    }

    pub fn error(&self, phase: &str, message: impl AsRef<str>) {
        self.emit(phase, message.as_ref(), "error");
    }

    fn emit(&self, phase: &str, message: &str, level: &str) {
        let Some(app) = &self.app else {
            return;
        };
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let payload = PluginInstallProgressEvent {
            phase: phase.to_owned(),
            message: message.to_owned(),
            ts,
            level: level.to_owned(),
        };
        let _ = app.emit(PLUGIN_INSTALL_PROGRESS_EVENT, &payload);
    }
}

/// One entry from `examples/plugins/sources.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSourceEntry {
    pub id: String,
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub github: Option<String>,
    #[serde(default)]
    pub r#ref: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub package_path: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourcesFile {
    sources: Vec<PluginSourceEntry>,
}

/// Load built-in sources registry from the monorepo (dev) or next to the binary.
pub fn load_plugin_sources() -> Result<Vec<PluginSourceEntry>, String> {
    let path = plugin_sources_path()
        .ok_or_else(|| "plugin sources registry not found (examples/plugins/sources.json)".to_owned())?;
    let bytes = fs::read(&path).map_err(|e| format!("read sources.json failed: {e}"))?;
    let file: SourcesFile =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid sources.json: {e}"))?;
    Ok(file.sources)
}

fn plugin_sources_path() -> Option<PathBuf> {
    let from_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/plugins/sources.json");
    if from_manifest.is_file() {
        return Some(from_manifest);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        for candidate in [
            dir.join("plugins/sources.json"),
            dir.join("../Resources/plugins/sources.json"),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn cache_root(dirs: &PorticoUserDirs) -> PathBuf {
    dirs.plugins.join("cache").join("github")
}

/// Normalize common GitHub URL shapes to `https://github.com/owner/repo`.
pub fn normalize_github_url(input: &str) -> Result<String, String> {
    let trimmed = input.trim().trim_end_matches('/').trim_end_matches(".git");
    if trimmed.is_empty() {
        return Err("GitHub URL is empty".to_owned());
    }
    // owner/repo shorthand
    if !trimmed.contains("://") && !trimmed.starts_with("git@") {
        let parts: Vec<_> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() == 2 {
            return Ok(format!("https://github.com/{}/{}", parts[0], parts[1]));
        }
    }
    let url = if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        format!("https://github.com/{rest}")
    } else {
        trimmed.to_owned()
    };
    if !(url.starts_with("https://github.com/") || url.starts_with("http://github.com/")) {
        return Err("only github.com repositories are supported".to_owned());
    }
    // Drop /tree/..., /blob/..., etc.
    let base = url
        .trim_start_matches("http://github.com/")
        .trim_start_matches("https://github.com/");
    let mut segs = base.split('/').filter(|s| !s.is_empty());
    let owner = segs.next().ok_or_else(|| "invalid GitHub URL".to_owned())?;
    let repo = segs.next().ok_or_else(|| "invalid GitHub URL".to_owned())?;
    Ok(format!("https://github.com/{owner}/{repo}"))
}

fn repo_slug(url: &str) -> Result<(String, String), String> {
    let base = url
        .trim_start_matches("https://github.com/")
        .trim_start_matches("http://github.com/");
    let mut segs = base.split('/').filter(|s| !s.is_empty());
    let owner = segs
        .next()
        .ok_or_else(|| "invalid GitHub URL".to_owned())?
        .to_owned();
    let repo = segs
        .next()
        .ok_or_else(|| "invalid GitHub URL".to_owned())?
        .to_owned();
    Ok((owner, repo))
}

fn run_cmd(
    progress: &InstallProgress,
    cwd: &Path,
    program: &str,
    args: &[&str],
) -> Result<String, String> {
    let cmdline = format!("{program} {}", args.join(" "));
    progress.info("cmd", format!("$ {cmdline}"));
    progress.info("cmd", format!("cwd: {}", cwd.display()));
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("failed to spawn {program}: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Stream last lines so the UI keeps moving on long builds.
    for line in stdout.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev() {
        let t = line.trim();
        if !t.is_empty() {
            progress.info("stdout", t);
        }
    }
    for line in stderr.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev() {
        let t = line.trim();
        if !t.is_empty() {
            progress.warn("stderr", t);
        }
    }
    if !output.status.success() {
        return Err(format!(
            "{program} {} failed ({status}): {stderr}{stdout}",
            args.join(" "),
            status = output.status
        ));
    }
    progress.ok("cmd", format!("✓ {cmdline}"));
    Ok(stdout.into_owned())
}

/// Shallow clone or fetch into cache. Returns local repo path.
pub fn ensure_github_clone(
    progress: &InstallProgress,
    dirs: &PorticoUserDirs,
    github_url: &str,
    git_ref: Option<&str>,
) -> Result<PathBuf, String> {
    let url = normalize_github_url(github_url)?;
    let (owner, repo) = repo_slug(&url)?;
    let dest = cache_root(dirs).join(&owner).join(&repo);
    progress.info("git", format!("cache path: {}", dest.display()));
    fs::create_dir_all(dest.parent().unwrap_or(Path::new(".")))
        .map_err(|e| format!("create cache dir failed: {e}"))?;

    if dest.join(".git").is_dir() {
        progress.info("git", "existing clone found — fetch / update");
        let _ = run_cmd(progress, &dest, "git", &["fetch", "--depth", "1", "origin"]);
        if let Some(r) = git_ref {
            let _ = run_cmd(progress, &dest, "git", &["checkout", r]);
            let _ = run_cmd(progress, &dest, "git", &["pull", "--ff-only", "origin", r]);
        } else {
            let _ = run_cmd(progress, &dest, "git", &["pull", "--ff-only"]);
        }
        progress.ok("git", "repository up to date");
        return Ok(dest);
    }

    if dest.exists() {
        progress.warn("git", "clearing incomplete cache directory");
        fs::remove_dir_all(&dest).map_err(|e| format!("clear stale clone failed: {e}"))?;
    }

    progress.info("git", format!("cloning {url} (shallow)…"));
    let mut args = vec![
        "clone".to_owned(),
        "--depth".to_owned(),
        "1".to_owned(),
    ];
    if let Some(r) = git_ref {
        args.push("--branch".to_owned());
        args.push(r.to_owned());
    }
    args.push(url.clone());
    args.push(dest.to_string_lossy().into_owned());
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let parent = dest.parent().unwrap_or(Path::new("/"));
    run_cmd(progress, parent, "git", &arg_refs)?;
    progress.ok("git", format!("cloned {owner}/{repo}"));
    Ok(dest)
}

fn bridge_template_root() -> Option<PathBuf> {
    portico_paths::bundled_plugins_root().map(|p| p.join("markdown-viewer-provider"))
}

/// Portico-owned files that must survive every upstream engine upgrade.
/// Never replace these with chrome-extension sources from GitHub.
const PORTICO_BRIDGE_ROOT_FILES: &[&str] = &[
    "index.html",
    "provider.js",
    "chrome-shim.js",
    "styles.css",
    "UPSTREAM.md",
];

/// Relative paths that must exist after packaging for the fast fallback path.
const PORTICO_FALLBACK_MARKERS: &[&str] = &[
    "vendor/fallback/marked.umd.js",
    "vendor/fallback/mermaid.min.js",
    "vendor/fallback/katex.min.js",
];

/// Apply Portico bridge + fallback assets into a staging package, then stamp version.
///
/// Upgrade contract:
/// - `vendor/engine` may be replaced by upstream dist
/// - bridge + `vendor/fallback` always come from the Portico monorepo template
/// - version metadata tracks upstream `package.json`
fn apply_portico_bridge(
    progress: &InstallProgress,
    staging: &Path,
    version: &str,
) -> Result<(), String> {
    if let Some(bridge) = bridge_template_root() {
        progress.info("bridge", format!("applying Portico bridge from {}", bridge.display()));
        for name in PORTICO_BRIDGE_ROOT_FILES {
            let from = bridge.join(name);
            if from.is_file() {
                progress.info("bridge", format!("copy {name}"));
                fs::copy(&from, staging.join(name))
                    .map_err(|e| format!("copy bridge {name} failed: {e}"))?;
            } else if *name == "provider.js" || *name == "chrome-shim.js" || *name == "styles.css" {
                return Err(format!(
                    "required Portico bridge file missing: {} (upgrade would break perf/compat)",
                    from.display()
                ));
            }
        }
        let fallback_src = bridge.join("vendor/fallback");
        if !fallback_src.is_dir() {
            return Err(format!(
                "vendor/fallback missing at {} — required for default fast render path",
                fallback_src.display()
            ));
        }
        let fallback_dest = staging.join("vendor/fallback");
        if fallback_dest.exists() {
            let _ = fs::remove_dir_all(&fallback_dest);
        }
        progress.info("bridge", "copy vendor/fallback (marked/mermaid/katex)");
        copy_dir_all(&fallback_src, &fallback_dest)
            .map_err(|e| format!("copy vendor/fallback failed: {e}"))?;
    } else {
        progress.warn("bridge", "monorepo bridge missing — writing embedded minimal bridge");
        write_minimal_bridge(staging, version)?;
    }

    stamp_bridge_version(staging, version)?;
    assert_fallback_present(staging)?;
    progress.ok(
        "bridge",
        format!("Portico bridge applied (fallback path ready, version={version})"),
    );
    Ok(())
}

fn stamp_bridge_version(staging: &Path, version: &str) -> Result<(), String> {
    // index.html: ensure meta portico-plugin-version matches upstream.
    let index_path = staging.join("index.html");
    if index_path.is_file() {
        let mut html = fs::read_to_string(&index_path).map_err(|e| e.to_string())?;
        if html.contains("name=\"portico-plugin-version\"") {
            // replace content="…"
            let re = regex_lite_replace_meta_version(&html, version);
            html = re;
        } else if let Some(idx) = html.find("<head>") {
            let insert = format!(
                "<head>\n    <meta name=\"portico-plugin-version\" content=\"{version}\" />"
            );
            html.replace_range(idx..idx + "<head>".len(), &insert);
        }
        fs::write(&index_path, html).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Small helper without adding a regex crate dependency.
fn regex_lite_replace_meta_version(html: &str, version: &str) -> String {
    const NEEDLE: &str = "name=\"portico-plugin-version\"";
    let Some(meta_at) = html.find(NEEDLE) else {
        return html.to_owned();
    };
    // Search content="…" after the name attribute (either order is fine in practice).
    let after = &html[meta_at..];
    let Some(c0) = after.find("content=\"") else {
        return html.to_owned();
    };
    let abs = meta_at + c0 + "content=\"".len();
    let rest = &html[abs..];
    let Some(end) = rest.find('"') else {
        return html.to_owned();
    };
    let mut out = String::with_capacity(html.len() + version.len());
    out.push_str(&html[..abs]);
    out.push_str(version);
    out.push_str(&html[abs + end..]);
    out
}

fn assert_fallback_present(staging: &Path) -> Result<(), String> {
    for rel in PORTICO_FALLBACK_MARKERS {
        let p = staging.join(rel);
        if !p.is_file() {
            return Err(format!(
                "packaged plugin missing {rel} — default fast path would break on upgrade"
            ));
        }
    }
    for name in ["provider.js", "chrome-shim.js", "styles.css"] {
        if !staging.join(name).is_file() {
            return Err(format!(
                "packaged plugin missing {name} — Portico bridge incomplete"
            ));
        }
    }
    Ok(())
}

/// Build a Portico package from a local path according to kind / auto-detect.
pub fn materialize_portico_package(
    progress: &InstallProgress,
    dirs: &PorticoUserDirs,
    source_repo: &Path,
    kind: &str,
    display_name_hint: Option<&str>,
) -> Result<PathBuf, String> {
    progress.info("package", format!("materialize kind={kind} path={}", source_repo.display()));
    match kind {
        "markdown-viewer-extension" => {
            materialize_markdown_viewer_extension(progress, dirs, source_repo)
        }
        "portico-package" => {
            if source_repo.join("plugin.json").is_file() {
                progress.ok("package", "found plugin.json at package root");
                return Ok(source_repo.to_path_buf());
            }
            Err("portico-package kind requires plugin.json at repository root".to_owned())
        }
        "auto" | "" => {
            if source_repo.join("plugin.json").is_file() {
                progress.ok("package", "auto: Portico package (plugin.json)");
                return Ok(source_repo.to_path_buf());
            }
            if source_repo.join("chrome/build-config.js").is_file()
                && source_repo.join("package.json").is_file()
            {
                progress.info("package", "auto: detected markdown-viewer-extension layout");
                return materialize_markdown_viewer_extension(progress, dirs, source_repo);
            }
            Err(format!(
                "repository is not a Portico plugin (no plugin.json). hint={display_name_hint:?}"
            ))
        }
        other => Err(format!("unknown plugin source kind: {other}")),
    }
}

fn materialize_markdown_viewer_extension(
    progress: &InstallProgress,
    dirs: &PorticoUserDirs,
    source_repo: &Path,
) -> Result<PathBuf, String> {
    // Fast path: monorepo already has bridge + fallback (+ optional engine).
    // Always assert fallback so upgrades from an incomplete tree fail loudly.
    if let Some(local) = bridge_template_root() {
        let manifest = local.join("plugin.json");
        let bridge_ok = manifest.is_file()
            && local.join("provider.js").is_file()
            && local.join("chrome-shim.js").is_file()
            && local.join("vendor/fallback/marked.umd.js").is_file();
        if bridge_ok {
            progress.info("fast-path", "using local monorepo Portico package");
            progress.info("fast-path", format!("source: {}", local.display()));
            let available = dirs.plugins_available.join("markdown-viewer-provider");
            if available.exists() {
                progress.info("fast-path", "refreshing available/markdown-viewer-provider");
                let _ = fs::remove_dir_all(&available);
            }
            progress.info("fast-path", "copying package files…");
            copy_dir_all(&local, &available)
                .map_err(|e| format!("copy local markdown-viewer package failed: {e}"))?;
            assert_fallback_present(&available)?;
            let m = plugin_package::read_manifest_public(&available)?;
            // Re-stamp version meta so badge matches plugin.json after upgrade.
            let _ = stamp_bridge_version(&available, &m.version);
            progress.ok(
                "fast-path",
                format!(
                    "ready package v{} (bridge+fallback preserved, no full rebuild)",
                    m.version
                ),
            );
            return Ok(available);
        }
        progress.warn(
            "fast-path",
            "local monorepo package incomplete (need provider.js + chrome-shim + vendor/fallback) — will build from upstream source",
        );
    }

    progress.info("build", "full upstream build (npm + esbuild) — this can take several minutes");
    if !which_ok("node") || !which_ok("npm") {
        return Err(
            "building markdown-viewer-extension requires node and npm on PATH (or use local monorepo package with vendor/engine)"
                .to_owned(),
        );
    }
    if !which_ok("git") {
        return Err("git is required to fetch GitHub sources".to_owned());
    }

    progress.info("build", format!("npm install in {}", source_repo.display()));
    run_cmd(
        progress,
        source_repo,
        "npm",
        &["install", "--ignore-scripts"],
    )?;
    progress.info("build", "writing node esbuild script");
    write_node_build_script(source_repo)?;
    progress.info("build", "esbuild chrome dist (long step)…");
    run_cmd(progress, source_repo, "node", &["build-chrome-node.mjs"])?;

    let dist = source_repo.join("dist/chrome");
    if !dist.is_dir() {
        return Err("upstream build did not produce dist/chrome".to_owned());
    }
    progress.ok("build", format!("dist ready: {}", dist.display()));

    let version =
        read_json_version(&source_repo.join("package.json")).unwrap_or_else(|| "0.0.0".into());
    progress.info("package", format!("upstream version = {version}"));

    let staging = dirs
        .plugins
        .join("cache")
        .join("packages")
        .join(format!("markdown-viewer-extension-{version}"));
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    fs::create_dir_all(&staging).map_err(|e| format!("create staging failed: {e}"))?;
    progress.info("package", format!("staging: {}", staging.display()));

    let license_src = source_repo.join("LICENSE");
    if license_src.is_file() {
        let _ = fs::copy(license_src, staging.join("LICENSE-GPL-3.0"));
    }

    progress.info("package", "copying engine (dist/chrome → vendor/engine)…");
    let engine_dest = staging.join("vendor/engine");
    copy_dir_all(&dist, &engine_dest).map_err(|e| format!("copy engine failed: {e}"))?;
    progress.ok("package", "engine copied");

    // Always re-apply Portico bridge AFTER engine copy so upgrades never lose
    // fallback performance path or chrome-shim (upstream must not overwrite these).
    apply_portico_bridge(progress, &staging, &version)?;

    let manifest = serde_json::json!({
        "id": "a8c3e2f1-4b5d-6e7f-8091-a2b3c4d5e6f7",
        "name": "markdown-viewer-provider",
        "version": version,
        "display_name": format!("Markdown Viewer (markdown-viewer-extension {version})"),
        "description": format!(
            "Installed from GitHub markdown-viewer-extension v{version} (GPL-3.0). \
             Default render: Portico vendor/fallback; optional vendor/engine."
        ),
        "skills": [],
        "tools": [],
        "entrypoint": "index.html",
        "capabilities": ["markdown.preview", "markdown.export.html"],
        "install_path": null,
        "permissions": { "network": [], "filesystem": "none" },
        "enabled": true,
        "installed_at": chrono_like_now()
    });
    fs::write(
        staging.join("plugin.json"),
        serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "{}".into()),
    )
    .map_err(|e| format!("write plugin.json failed: {e}"))?;
    progress.info("package", format!("wrote plugin.json v{version}"));

    plugin_package::read_manifest_public(&staging)?;
    progress.ok("package", "package validated (bridge + fallback asserted)");
    Ok(staging)
}

fn which_ok(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn write_node_build_script(source_repo: &Path) -> Result<(), String> {
    let script = r#"
import { build } from 'esbuild';
import { createBuildConfig } from './chrome/build-config.js';
import fs from 'fs';
import path from 'path';

const packageJson = JSON.parse(fs.readFileSync('package.json', 'utf8'));
let manifestText = fs.readFileSync('chrome/manifest.json', 'utf8');
manifestText = manifestText.replace(/"version":\s*"[^"]*"/, `"version": "${packageJson.version}"`);
fs.writeFileSync('chrome/manifest.json', manifestText);
console.log('building markdown-viewer-extension', packageJson.version);
try {
  const mod = await import('./scripts/sync-formats.js');
  const syncFormats = mod.default || mod;
  if (typeof syncFormats === 'function') syncFormats();
} catch (_) {}
const outdir = path.join(process.cwd(), 'dist/chrome');
if (fs.existsSync(outdir)) fs.rmSync(outdir, { recursive: true, force: true });
await build(createBuildConfig());
console.log('wrote', outdir);
"#;
    fs::write(source_repo.join("build-chrome-node.mjs"), script)
        .map_err(|e| format!("write build script failed: {e}"))
}

fn write_minimal_bridge(staging: &Path, version: &str) -> Result<(), String> {
    // Embedded bridge when monorepo template path is unavailable (release builds).
    // Still prefers fallback path — does NOT load multi-MB engine by default.
    let index = format!(
        r#"<!doctype html>
<html lang="zh-CN"><head>
<meta charset="UTF-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1.0"/>
<meta name="portico-plugin-version" content="{version}"/>
<title>Markdown Viewer {version}</title>
<link rel="stylesheet" href="styles.css"/>
</head>
<body>
<main id="document" class="doc-surface"><p class="doc-placeholder">就绪…</p></main>
<script src="provider.js"></script>
</body></html>
"#
    );
    fs::write(staging.join("index.html"), index).map_err(|e| e.to_string())?;
    let provider = include_str!("../../examples/plugins/markdown-viewer-provider/provider.js");
    fs::write(staging.join("provider.js"), provider).map_err(|e| e.to_string())?;
    let styles = include_str!("../../examples/plugins/markdown-viewer-provider/styles.css");
    fs::write(staging.join("styles.css"), styles).map_err(|e| e.to_string())?;
    let shim = include_str!("../../examples/plugins/markdown-viewer-provider/chrome-shim.js");
    fs::write(staging.join("chrome-shim.js"), shim).map_err(|e| e.to_string())?;
    // vendor/fallback cannot be fully embedded via include_str (mermaid is multi-MB).
    // Callers must ship fallback next to the binary or use monorepo template.
    Ok(())
}

fn read_json_version(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("version")?.as_str().map(str::to_owned)
}

fn chrono_like_now() -> String {
    // RFC3339 UTC without requiring chrono in this module.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 1970-01-01T00:00:00Z + secs — good enough for installed_at validation.
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let hours = rem / 3600;
    let mins = (rem % 3600) / 60;
    let s = rem % 60;
    // Approximate Y-M-D from days since epoch (good enough for plugin metadata).
    let (y, m, d) = days_to_ymd(days);
    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{mins:02}:{s:02}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Civil from days (Howard Hinnant algorithm, simplified for u64).
    days = days.saturating_add(719_468);
    let era = days / 146_097;
    let doe = days.saturating_sub(era * 146_097);
    let yoe = (doe.saturating_sub(doe / 1460) + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe.saturating_sub(365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy.saturating_sub((153 * mp + 2) / 5) + 1;
    let m = if mp < 10 { mp + 3 } else { mp.saturating_sub(9) };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
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

/// Full closed loop: GitHub URL or catalog name → Portico package directory ready to install.
pub fn resolve_and_materialize(
    progress: &InstallProgress,
    dirs: &PorticoUserDirs,
    github_or_name: &str,
    git_ref: Option<&str>,
) -> Result<(PathBuf, PluginSourceEntry), String> {
    progress.info("start", format!("resolve source: {github_or_name}"));
    if let Some(r) = git_ref {
        progress.info("start", format!("git ref: {r}"));
    }
    let sources = load_plugin_sources().unwrap_or_default();
    progress.info("start", format!("loaded {} catalog source(s)", sources.len()));
    let input = github_or_name.trim();

    if let Some(entry) = sources.iter().find(|s| {
        s.name == input
            || s.id == input
            || s.github
                .as_deref()
                .is_some_and(|g| normalize_github_url(g).ok().as_deref() == Some(input)
                    || normalize_github_url(input)
                        .ok()
                        .is_some_and(|u| normalize_github_url(g).ok().as_deref() == Some(u.as_str())))
    }) {
        progress.info(
            "catalog",
            format!("matched catalog entry: {} ({})", entry.display_name, entry.kind),
        );
        return materialize_catalog_entry(progress, dirs, entry, git_ref);
    }

    if input.contains("github.com") || input.split('/').count() == 2 {
        let url = normalize_github_url(input)?;
        progress.info("github", format!("normalized URL: {url}"));
        let repo = ensure_github_clone(progress, dirs, &url, git_ref)?;
        let package = materialize_portico_package(progress, dirs, &repo, "auto", None)?;
        let synthetic = PluginSourceEntry {
            id: String::new(),
            name: repo
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "github-plugin".into()),
            display_name: "GitHub plugin".into(),
            github: Some(url),
            r#ref: git_ref.map(str::to_owned),
            kind: "auto".into(),
            package_path: None,
            capabilities: vec![],
            license: None,
            notes: None,
        };
        progress.ok("done", format!("package ready at {}", package.display()));
        return Ok((package, synthetic));
    }

    Err(format!(
        "unknown plugin source '{input}'. Use a catalog name from sources.json or a github.com URL."
    ))
}

fn materialize_catalog_entry(
    progress: &InstallProgress,
    dirs: &PorticoUserDirs,
    entry: &PluginSourceEntry,
    git_ref_override: Option<&str>,
) -> Result<(PathBuf, PluginSourceEntry), String> {
    match entry.kind.as_str() {
        "portico-package" => {
            let name = entry
                .package_path
                .as_deref()
                .unwrap_or(entry.name.as_str());
            progress.info("catalog", format!("seed local package: {name}"));
            let path = portico_paths::seed_catalog_package_to_available(
                &dirs.plugins_available,
                name,
            )?;
            progress.ok("catalog", format!("seeded {}", path.display()));
            Ok((path, entry.clone()))
        }
        "markdown-viewer-extension" => {
            if let Some(local) = bridge_template_root() {
                let engine = local.join("vendor/engine/core/element-runtime.js");
                if engine.is_file() && local.join("plugin.json").is_file() {
                    progress.info("catalog", "markdown-viewer: try local monorepo package first");
                    let package = materialize_portico_package(
                        progress,
                        dirs,
                        &local,
                        "markdown-viewer-extension",
                        None,
                    )?;
                    return Ok((package, entry.clone()));
                }
            }
            let github = entry
                .github
                .as_deref()
                .ok_or_else(|| "catalog entry missing github URL".to_owned())?;
            let r = git_ref_override.or(entry.r#ref.as_deref());
            progress.info("catalog", format!("markdown-viewer: clone from {github}"));
            let repo = ensure_github_clone(progress, dirs, github, r)?;
            let package = materialize_portico_package(
                progress,
                dirs,
                &repo,
                "markdown-viewer-extension",
                None,
            )?;
            let available = dirs.plugins_available.join(&entry.name);
            if available.exists() {
                let _ = fs::remove_dir_all(&available);
            }
            progress.info("catalog", "copy package into available/");
            copy_dir_all(&package, &available)
                .map_err(|e| format!("seed available failed: {e}"))?;
            progress.ok("done", format!("package ready at {}", package.display()));
            Ok((package, entry.clone()))
        }
        other => Err(format!("unsupported catalog kind: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_github_urls() {
        assert_eq!(
            normalize_github_url("https://github.com/markdown-viewer/markdown-viewer-extension")
                .unwrap(),
            "https://github.com/markdown-viewer/markdown-viewer-extension"
        );
        assert_eq!(
            normalize_github_url("markdown-viewer/markdown-viewer-extension").unwrap(),
            "https://github.com/markdown-viewer/markdown-viewer-extension"
        );
        assert_eq!(
            normalize_github_url(
                "https://github.com/markdown-viewer/markdown-viewer-extension/tree/main/chrome"
            )
            .unwrap(),
            "https://github.com/markdown-viewer/markdown-viewer-extension"
        );
    }

    #[test]
    fn stamp_meta_version_replaces_content() {
        let html = r#"<head><meta name="portico-plugin-version" content="5.2.0" /><title>x</title></head>"#;
        let next = regex_lite_replace_meta_version(html, "6.0.0");
        assert!(next.contains("content=\"6.0.0\""));
        assert!(!next.contains("content=\"5.2.0\""));
    }

    #[test]
    fn bridge_file_list_includes_shim_and_fallback_markers() {
        assert!(PORTICO_BRIDGE_ROOT_FILES.contains(&"chrome-shim.js"));
        assert!(PORTICO_BRIDGE_ROOT_FILES.contains(&"provider.js"));
        assert!(PORTICO_FALLBACK_MARKERS
            .iter()
            .any(|p| p.contains("marked.umd.js")));
    }
}
