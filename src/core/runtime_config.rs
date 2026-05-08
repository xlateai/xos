use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default)]
    pub daemon_enabled: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            daemon_enabled: false,
        }
    }
}

fn config_path() -> Result<PathBuf, String> {
    let data_dir = crate::auth::auth_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir.join("config.json"))
}

pub fn load_or_init_runtime_config() -> Result<RuntimeConfig, String> {
    let path = config_path()?;
    if !path.exists() {
        let cfg = RuntimeConfig::default();
        write_runtime_config(&cfg)?;
        return Ok(cfg);
    }

    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    match serde_json::from_str::<RuntimeConfig>(&raw) {
        Ok(cfg) => Ok(cfg),
        Err(e) => Err(format!("invalid runtime config at {}: {e}", path.display())),
    }
}

pub fn write_runtime_config(cfg: &RuntimeConfig) -> Result<(), String> {
    let path = config_path()?;
    let raw = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| e.to_string())
}

pub fn daemon_enabled() -> Result<bool, String> {
    Ok(load_or_init_runtime_config()?.daemon_enabled)
}

pub fn set_daemon_enabled(enabled: bool) -> Result<(), String> {
    let mut cfg = load_or_init_runtime_config()?;
    cfg.daemon_enabled = enabled;
    write_runtime_config(&cfg)
}
