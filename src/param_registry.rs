use crate::dsp::shared::Shared;
use std::collections::HashMap;

/// Global parameter registry mapping fully-qualified names to Shared handles.
///
/// Keys follow the pattern `"{species}_{org_index}.cell{N}.{param}"`,
/// e.g. `"DRON_0.cell1.cutoff"`. Built on the control thread at spawn time,
/// cleaned up on kill. Used for MIDI CC→Shared dispatch.
pub struct ParamRegistry {
    /// All registered parameter handles keyed by fully-qualified name.
    params: HashMap<String, Shared>,
    /// Track which keys belong to each organism (for cleanup on kill).
    organism_keys: HashMap<u32, Vec<String>>,
}

impl ParamRegistry {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
            organism_keys: HashMap::new(),
        }
    }

    /// Register all shared handles for an organism.
    /// `prefix` should be e.g. `"DRON_0"`.
    /// `handles` maps local keys like `"cell0.freq"` to Shared values.
    pub fn register(&mut self, org_id: u32, prefix: &str, handles: &HashMap<String, Shared>) {
        let mut keys = Vec::with_capacity(handles.len());
        for (local_key, handle) in handles {
            let full_key = format!("{}.{}", prefix, local_key);
            self.params.insert(full_key.clone(), handle.clone());
            keys.push(full_key);
        }
        self.organism_keys.insert(org_id, keys);
    }

    /// Remove all parameters belonging to an organism.
    pub fn unregister(&mut self, org_id: u32) {
        if let Some(keys) = self.organism_keys.remove(&org_id) {
            for key in keys {
                self.params.remove(&key);
            }
        }
    }

    /// Look up a Shared handle by fully-qualified key.
    pub fn get(&self, key: &str) -> Option<&Shared> {
        self.params.get(key)
    }

    /// Set a parameter value by fully-qualified key. Returns true if found.
    pub fn set(&self, key: &str, value: f32) -> bool {
        if let Some(handle) = self.params.get(key) {
            handle.set(value);
            true
        } else {
            false
        }
    }

    /// Number of registered parameters.
    pub fn len(&self) -> usize {
        self.params.len()
    }

    /// Iterate all keys (for debug/UI display).
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.params.keys()
    }

    /// Get all keys for a specific organism (for CC learn target selection).
    pub fn organism_params(&self, org_id: u32) -> Option<&[String]> {
        self.organism_keys.get(&org_id).map(|v| v.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::shared::shared;

    #[test]
    fn register_and_lookup() {
        let mut reg = ParamRegistry::new();
        let mut handles = HashMap::new();
        handles.insert("cell0.freq".to_string(), shared(440.0));
        handles.insert("cell1.cutoff".to_string(), shared(0.5));
        reg.register(1, "DRON_0", &handles);

        assert_eq!(reg.len(), 2);
        assert!(reg.get("DRON_0.cell0.freq").is_some());
        assert!(reg.get("DRON_0.cell1.cutoff").is_some());
        assert!(reg.get("DRON_0.cell2.gain").is_none());
    }

    #[test]
    fn set_value() {
        let mut reg = ParamRegistry::new();
        let mut handles = HashMap::new();
        let h = shared(0.0);
        handles.insert("cell0.freq".to_string(), h.clone());
        reg.register(1, "DRON_0", &handles);

        assert!(reg.set("DRON_0.cell0.freq", 880.0));
        assert!((h.value() - 880.0).abs() < f32::EPSILON);
        assert!(!reg.set("NONEXISTENT", 1.0));
    }

    #[test]
    fn unregister_cleans_up() {
        let mut reg = ParamRegistry::new();
        let mut handles = HashMap::new();
        handles.insert("cell0.freq".to_string(), shared(440.0));
        reg.register(1, "DRON_0", &handles);
        assert_eq!(reg.len(), 1);

        reg.unregister(1);
        assert_eq!(reg.len(), 0);
        assert!(reg.get("DRON_0.cell0.freq").is_none());
    }

    #[test]
    fn multiple_organisms() {
        let mut reg = ParamRegistry::new();
        let mut h1 = HashMap::new();
        h1.insert("cell0.freq".to_string(), shared(440.0));
        reg.register(1, "DRON_0", &h1);

        let mut h2 = HashMap::new();
        h2.insert("cell0.cutoff".to_string(), shared(0.7));
        reg.register(2, "ACID_0", &h2);

        assert_eq!(reg.len(), 2);
        reg.unregister(1);
        assert_eq!(reg.len(), 1);
        assert!(reg.get("ACID_0.cell0.cutoff").is_some());
    }
}
