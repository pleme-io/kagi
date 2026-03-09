use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KagiConfig {
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub clipboard: ClipboardConfig,
    #[serde(default)]
    pub appearance: AppearanceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// 1Password Connect server URL (if using Connect API)
    pub connect_url: Option<String>,
    /// 1Password Connect token
    pub connect_token: Option<String>,
    /// Path to `op` CLI binary (fallback)
    #[serde(default = "default_op_path")]
    pub op_path: String,
    /// 1Password service account token (for `op` CLI)
    pub service_account_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardConfig {
    /// Seconds before clipboard is cleared after copying a secret
    #[serde(default = "default_clear_timeout")]
    pub clear_timeout_secs: u32,
    /// Whether to auto-clear clipboard
    #[serde(default = "default_auto_clear")]
    pub auto_clear: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    #[serde(default = "default_bg")]
    pub background: String,
    #[serde(default = "default_fg")]
    pub foreground: String,
    #[serde(default = "default_accent")]
    pub accent: String,
}

impl Default for KagiConfig {
    fn default() -> Self {
        Self {
            api: ApiConfig::default(),
            clipboard: ClipboardConfig::default(),
            appearance: AppearanceConfig::default(),
        }
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            connect_url: None,
            connect_token: None,
            op_path: default_op_path(),
            service_account_token: None,
        }
    }
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self { clear_timeout_secs: default_clear_timeout(), auto_clear: default_auto_clear() }
    }
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self { background: default_bg(), foreground: default_fg(), accent: default_accent() }
    }
}

fn default_op_path() -> String { "op".into() }
fn default_clear_timeout() -> u32 { 30 }
fn default_auto_clear() -> bool { true }
fn default_bg() -> String { "#2e3440".into() }
fn default_fg() -> String { "#eceff4".into() }
fn default_accent() -> String { "#88c0d0".into() }

pub fn load(override_path: &Option<PathBuf>) -> anyhow::Result<KagiConfig> {
    let path = match override_path {
        Some(p) => p.clone(),
        None => match shikumi::ConfigDiscovery::new("kagi")
            .env_override("KAGI_CONFIG")
            .discover()
        {
            Ok(p) => p,
            Err(_) => {
                tracing::info!("no config file found, using defaults");
                return Ok(KagiConfig::default());
            }
        },
    };

    let store = shikumi::ConfigStore::<KagiConfig>::load(&path, "KAGI_")?;
    Ok(KagiConfig::clone(&store.get()))
}
