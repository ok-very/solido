//! Application configuration — persistent settings for audio, graphics, MIDI, and window.
//!
//! Saved to `solido-settings.json` next to the executable.
//! Loaded at startup, graceful fallback to defaults if missing or corrupt.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ─── Config Structs ──────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SolidoConfig {
    pub audio: AudioConfig,
    pub graphics: GraphicsConfig,
    pub midi: MidiConfig,
    pub window: WindowConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    /// Output device name from cpal enumeration. None = system default.
    pub device_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicsConfig {
    /// Quality preset: "low", "medium", "high", "ultra".
    pub quality_preset: String,
    /// Biofield texture scale relative to viewport (0.5, 0.75, 1.0, 1.5).
    pub biofield_scale: f32,
    /// Fluid simulation scale relative to viewport.
    pub fluid_scale: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct MidiConfig {
    /// Last connected MIDI input port name for auto-reconnect.
    pub input_port: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}

// ─── Defaults ────────────────────────────────────────────────────────

impl Default for SolidoConfig {
    fn default() -> Self {
        Self {
            audio: AudioConfig::default(),
            graphics: GraphicsConfig::default(),
            midi: MidiConfig::default(),
            window: WindowConfig::default(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self { device_name: None }
    }
}

impl Default for GraphicsConfig {
    fn default() -> Self {
        Self {
            quality_preset: "high".into(),
            biofield_scale: 1.0,
            fluid_scale: 0.5,
        }
    }
}

impl Default for MidiConfig {
    fn default() -> Self {
        Self { input_port: None }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 1200.0,
            height: 700.0,
            maximized: false,
        }
    }
}

// ─── Quality Presets ─────────────────────────────────────────────────

/// Map a quality preset name to (biofield_scale, fluid_scale).
pub fn quality_scales(preset: &str) -> (f32, f32) {
    match preset {
        "low" => (0.5, 0.25),
        "medium" => (0.75, 0.375),
        "high" => (1.0, 0.5),
        "ultra" => (1.5, 0.75),
        _ => (1.0, 0.5),
    }
}

pub const QUALITY_PRESETS: &[&str] = &["low", "medium", "high", "ultra"];

// ─── Load / Save ─────────────────────────────────────────────────────

/// Resolve the config file path next to the executable (or CWD fallback).
pub fn config_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("solido-settings.json")
}

/// Load config from disk. Returns default if file is missing or corrupt.
pub fn load() -> SolidoConfig {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|e| {
            log::warn!("Config parse error ({}): {e} — using defaults", path.display());
            SolidoConfig::default()
        }),
        Err(_) => {
            log::info!("No config at {} — using defaults", path.display());
            SolidoConfig::default()
        }
    }
}

/// Save config to disk. Logs warning on failure.
pub fn save(config: &SolidoConfig) {
    let path = config_path();
    match serde_json::to_string_pretty(config) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("Failed to save config to {}: {e}", path.display());
            } else {
                log::info!("Config saved to {}", path.display());
            }
        }
        Err(e) => log::warn!("Failed to serialize config: {e}"),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_round_trips() {
        let config = SolidoConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SolidoConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.graphics.quality_preset, "high");
        assert_eq!(parsed.window.width, 1200.0);
        assert!(parsed.audio.device_name.is_none());
    }

    #[test]
    fn quality_preset_mapping() {
        assert_eq!(quality_scales("low"), (0.5, 0.25));
        assert_eq!(quality_scales("high"), (1.0, 0.5));
        assert_eq!(quality_scales("ultra"), (1.5, 0.75));
        assert_eq!(quality_scales("unknown"), (1.0, 0.5)); // fallback
    }

    #[test]
    fn missing_fields_use_defaults() {
        let json = r#"{"audio": {}}"#;
        let config: SolidoConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.graphics.biofield_scale, 1.0);
        assert_eq!(config.window.height, 700.0);
    }
}
