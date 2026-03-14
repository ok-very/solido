//! Global sample registry — caches loaded audio as `Arc<Vec<f32>>`.
//!
//! DNA references samples by URI key (e.g., `"uiowa:marimba:c4:mf"`).
//! At spawn time, `SampleRegistry` resolves URI to `Arc<Vec<f32>>`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Global sample registry. Caches loaded WAV buffers keyed by URI string.
///
/// Constructed once on `SolidoApp`, passed to `OrganismDsp::from_dna()` at spawn time.
/// Thread safety: lives on the control thread only. Audio thread receives `Arc<Vec<f32>>`
/// clones — no locks needed.
pub struct SampleRegistry {
    cache: HashMap<String, Arc<Vec<f32>>>,
    sample_dir: PathBuf,
}

impl SampleRegistry {
    /// Create a new registry rooted at `sample_dir` (e.g., `"assets/samples"`).
    pub fn new(sample_dir: impl Into<PathBuf>) -> Self {
        SampleRegistry {
            cache: HashMap::new(),
            sample_dir: sample_dir.into(),
        }
    }

    /// Load a sample by URI, returning a shared reference to the buffer.
    ///
    /// On first call for a given URI, loads from disk and caches.
    /// Subsequent calls return the cached `Arc`. Returns `None` if the file
    /// cannot be found or decoded.
    pub fn load(&mut self, uri: &str) -> Option<Arc<Vec<f32>>> {
        // Check cache first
        if let Some(buf) = self.cache.get(uri) {
            return Some(Arc::clone(buf));
        }

        // Resolve URI → filesystem path
        let path = self.resolve_path(uri);
        if !path.exists() {
            log::warn!("SampleRegistry: file not found for URI '{}' at {:?}", uri, path);
            return None;
        }

        // Load and cache
        match load_wav_mono(&path) {
            Ok(data) => {
                log::info!(
                    "SampleRegistry: loaded {} samples for '{}' from {:?}",
                    data.len(),
                    uri,
                    path
                );
                let arc = Arc::new(data);
                self.cache.insert(uri.to_string(), Arc::clone(&arc));
                Some(arc)
            }
            Err(e) => {
                log::warn!("SampleRegistry: failed to load '{}': {}", uri, e);
                None
            }
        }
    }

    /// Get a previously loaded sample. Returns `None` if not cached.
    pub fn get(&self, uri: &str) -> Option<Arc<Vec<f32>>> {
        self.cache.get(uri).map(Arc::clone)
    }

    /// Resolve a URI to a filesystem path.
    ///
    /// URI scheme: `{collection}:{instrument}:{note}:{dynamic}`
    /// - `uiowa:marimba:c4:mf` → `{sample_dir}/uiowa/marimba/marimba.yarn.mf.C4.wav`
    /// - `uiowa:xylophone:c5:mf` → `{sample_dir}/uiowa/xylophone/xylophone.rosewood.mf.C5.wav`
    /// - `uiowa:bells:c6:mf` → `{sample_dir}/uiowa/bells/bells.brass.mf.C6.wav`
    pub fn resolve_path(&self, uri: &str) -> PathBuf {
        let parts: Vec<&str> = uri.split(':').collect();
        if parts.len() < 4 {
            // Fallback: treat as direct relative path
            return self.sample_dir.join(uri);
        }

        let collection = parts[0];
        let instrument = parts[1];
        let note = parts[2];
        let dynamic = parts[3];

        // Map instrument to mallet/material qualifier
        let qualifier = match instrument {
            "marimba" => "yarn",
            "xylophone" => "rosewood",
            "bells" => "brass",
            "vibraphone" => "motor-off",
            _ => "default",
        };

        // Capitalize note name for UIowa convention (e.g., "c4" → "C4")
        let note_cap = capitalize_note(note);

        let filename = format!("{}.{}.{}.{}.wav", instrument, qualifier, dynamic, note_cap);

        self.sample_dir
            .join(collection)
            .join(instrument)
            .join(filename)
    }

    /// Number of cached samples.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.cache.len()
    }
}

/// Capitalize a note name: "c4" → "C4", "fs5" → "Fs5"
fn capitalize_note(note: &str) -> String {
    let mut chars = note.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Load a WAV file and return normalized mono f32 samples.
///
/// Stereo files are downmixed to mono. Integer formats are normalized to [-1, 1].
fn load_wav_mono(path: &Path) -> Result<Vec<f32>, String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| format!("open failed: {e}"))?;
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .filter_map(|s| s.ok())
            .collect(),
        hound::SampleFormat::Int => {
            let max_val = (1u32 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_val)
                .collect()
        }
    };

    // Downmix stereo → mono if needed.
    let mono = if spec.channels == 2 {
        samples
            .chunks(2)
            .map(|c| {
                let l = c[0];
                let r = c.get(1).copied().unwrap_or(0.0);
                (l + r) * 0.5
            })
            .collect()
    } else {
        samples
    };

    Ok(mono)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_uiowa_marimba() {
        let reg = SampleRegistry::new("assets/samples");
        let path = reg.resolve_path("uiowa:marimba:c4:mf");
        assert!(path.ends_with("uiowa/marimba/marimba.yarn.mf.C4.wav")
            || path.to_string_lossy().contains("marimba.yarn.mf.C4.wav"));
    }

    #[test]
    fn resolve_uiowa_bells() {
        let reg = SampleRegistry::new("assets/samples");
        let path = reg.resolve_path("uiowa:bells:c6:mf");
        assert!(path.to_string_lossy().contains("bells.brass.mf.C6.wav"));
    }

    #[test]
    fn resolve_uiowa_xylophone() {
        let reg = SampleRegistry::new("assets/samples");
        let path = reg.resolve_path("uiowa:xylophone:c5:mf");
        assert!(path.to_string_lossy().contains("xylophone.rosewood.mf.C5.wav"));
    }

    #[test]
    fn capitalize_note_cases() {
        assert_eq!(capitalize_note("c4"), "C4");
        assert_eq!(capitalize_note("fs5"), "Fs5");
        assert_eq!(capitalize_note("Bb3"), "Bb3");
    }

    #[test]
    fn cache_returns_same_arc() {
        // Without actual WAV files, test the cache logic with a manual insert
        let mut reg = SampleRegistry::new("assets/samples");
        let data = Arc::new(vec![0.0f32; 100]);
        reg.cache.insert("test:sine:c4:mf".into(), Arc::clone(&data));

        let got = reg.get("test:sine:c4:mf").unwrap();
        assert!(Arc::ptr_eq(&data, &got), "should return same Arc");
    }

    #[test]
    fn load_missing_returns_none() {
        let mut reg = SampleRegistry::new("nonexistent/path");
        assert!(reg.load("uiowa:marimba:c4:mf").is_none());
    }
}
