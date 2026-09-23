//! Settings: CLI flag > environment variable > YAML config file > default.

use crate::gemini::{DEFAULT_BASE_URL, DEFAULT_MODEL};
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
}

pub const TEMPLATE: &str = "\
# acm (autocommit-cli) configuration. Keep this file private: chmod 600.
# Environment variables (GEMINI_API_KEY, AUTOCOMMIT_MODEL, GEMINI_BASE_URL)
# and the --model flag override these values.

# Free key: https://aistudio.google.com/apikey
api_key: \"\"

# model: gemini-3.1-flash-lite
# base_url: https://generativelanguage.googleapis.com
";

impl FileConfig {
    pub fn parse(text: &str) -> Result<Self> {
        let has_content = text
            .lines()
            .any(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'));
        if !has_content {
            return Ok(Self::default());
        }
        serde_saphyr::from_str(text).map_err(|e| anyhow!("{e}"))
    }

    /// Load `path`; a missing file is an empty config.
    pub fn load(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        };
        warn_if_readable_by_others(path);
        Self::parse(&text).with_context(|| format!("invalid config file {}", path.display()))
    }
}

#[cfg(unix)]
fn warn_if_readable_by_others(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path)
        && meta.permissions().mode() & 0o077 != 0
    {
        eprintln!(
            "acm: warning: {} is readable by other users; run `chmod 600 {}`",
            path.display(),
            path.display()
        );
    }
}

#[cfg(not(unix))]
fn warn_if_readable_by_others(_path: &Path) {}

#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

fn non_blank(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

impl Config {
    pub fn resolve(
        cli_model: Option<&str>,
        file: &FileConfig,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Self> {
        let pick = |cli: Option<String>, var: &str, from_file: &Option<String>| {
            non_blank(cli)
                .or_else(|| non_blank(env(var)))
                .or_else(|| non_blank(from_file.clone()))
        };
        let api_key = pick(None, "GEMINI_API_KEY", &file.api_key).context(
            "no Gemini API key: set GEMINI_API_KEY or `api_key` in the config file \
             (run `acm --init`; free key at https://aistudio.google.com/apikey)",
        )?;
        let model = pick(
            cli_model.map(str::to_string),
            "AUTOCOMMIT_MODEL",
            &file.model,
        )
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let base_url = pick(None, "GEMINI_BASE_URL", &file.base_url)
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        Ok(Config {
            api_key,
            model,
            base_url,
        })
    }
}

/// Config path for the current OS; see `default_path_for`.
pub fn default_path(env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    default_path_for(env, cfg!(windows))
}

/// 1. `$AUTOCOMMIT_CONFIG`
/// 2. `$XDG_CONFIG_HOME/autocommit/config.yaml`
/// 3. Linux/macOS: `$HOME/.config/autocommit/config.yaml`
///    Windows: `%APPDATA%\autocommit\config.yaml`, else
///    `%USERPROFILE%\.config\autocommit\config.yaml`. HOME is ignored on
///    Windows so Git Bash and PowerShell resolve to the same file.
pub fn default_path_for(env: impl Fn(&str) -> Option<String>, windows: bool) -> Option<PathBuf> {
    if let Some(p) = non_blank(env("AUTOCOMMIT_CONFIG")) {
        return Some(PathBuf::from(p));
    }
    let base = non_blank(env("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| {
            if windows {
                non_blank(env("APPDATA")).map(PathBuf::from).or_else(|| {
                    non_blank(env("USERPROFILE")).map(|h| PathBuf::from(h).join(".config"))
                })
            } else {
                non_blank(env("HOME")).map(|h| PathBuf::from(h).join(".config"))
            }
        })?;
    Some(base.join("autocommit").join("config.yaml"))
}

/// Create `path` with the template, mode 0600; refuse to overwrite.
pub fn init(path: &Path) -> Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            anyhow!("{} already exists; edit it instead", path.display())
        } else {
            anyhow!("cannot create {}: {e}", path.display())
        }
    })?;
    file.write_all(TEMPLATE.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests;
