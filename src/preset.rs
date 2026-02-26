use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A snapshot of the musical performance state that can be saved/loaded.
/// Captures raga, tala, tempo, and gravity parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub raga: String,
    pub tala: String,
    pub tempo_bpm: f64,
    pub pitch_gravity: f32,
    pub rhythm_gravity: f32,
    pub gamaka_depth: f32,
    pub morph_speed: f32,
    #[serde(default)]
    pub manual_gravity: bool,
}

impl Default for Preset {
    fn default() -> Self {
        Self {
            name: "untitled".into(),
            raga: "bhairav".into(),
            tala: "teentaal".into(),
            tempo_bpm: 120.0,
            pitch_gravity: 0.5,
            rhythm_gravity: 0.5,
            gamaka_depth: 0.0,
            morph_speed: 0.5,
            manual_gravity: false,
        }
    }
}

/// Save preset to a JSON file.
pub fn save(preset: &Preset, path: &Path) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(preset)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

/// Load preset from a JSON file.
pub fn load(path: &Path) -> std::io::Result<Preset> {
    let json = std::fs::read_to_string(path)?;
    serde_json::from_str(&json)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Scan a directory for preset JSON files, returning (name, path) pairs sorted by name.
pub fn scan_presets(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut presets = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(p) = load(&path) {
                    presets.push((p.name.clone(), path));
                }
            }
        }
    }
    presets.sort_by(|a, b| a.0.cmp(&b.0));
    presets
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn preset_roundtrip_save_load() {
        let dir = std::env::temp_dir().join("solido_preset_test");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_roundtrip.json");

        let preset = Preset {
            name: "test".into(),
            raga: "yaman".into(),
            tala: "rupak".into(),
            tempo_bpm: 90.0,
            pitch_gravity: 0.3,
            rhythm_gravity: 0.6,
            gamaka_depth: 0.1,
            morph_speed: 1.2,
            manual_gravity: true,
        };

        save(&preset, &path).expect("save failed");
        let loaded = load(&path).expect("load failed");

        assert_eq!(loaded.name, "test");
        assert_eq!(loaded.raga, "yaman");
        assert_eq!(loaded.tala, "rupak");
        assert!((loaded.tempo_bpm - 90.0).abs() < 0.01);
        assert!((loaded.pitch_gravity - 0.3).abs() < 0.001);
        assert!(loaded.manual_gravity);

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn scan_presets_finds_files() {
        let dir = std::env::temp_dir().join("solido_preset_scan_test");
        let _ = fs::create_dir_all(&dir);

        // Create two presets
        let p1 = Preset {
            name: "alpha".into(),
            ..Default::default()
        };
        let p2 = Preset {
            name: "beta".into(),
            ..Default::default()
        };
        save(&p1, &dir.join("alpha.json")).unwrap();
        save(&p2, &dir.join("beta.json")).unwrap();

        let found = scan_presets(&dir);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, "alpha");
        assert_eq!(found[1].0, "beta");

        let _ = fs::remove_file(dir.join("alpha.json"));
        let _ = fs::remove_file(dir.join("beta.json"));
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn default_preset_is_valid() {
        let p = Preset::default();
        assert_eq!(p.name, "untitled");
        assert_eq!(p.raga, "bhairav");
        assert_eq!(p.tala, "teentaal");
        assert!((p.tempo_bpm - 120.0).abs() < 0.01);
        assert!(!p.manual_gravity);
    }
}
