pub mod gamaka;
pub mod gravity_control;
pub mod pitch_gravity;
pub mod raga;
pub mod rhythm_gravity;
pub mod scala;
pub mod scale_morph;

use std::collections::HashMap;

use scala::TuningSystem;

const SCL_12TET: &str = include_str!("../../assets/scales/12tet.scl");
const SCL_5LIMIT_JI: &str = include_str!("../../assets/scales/5limit_ji.scl");
const SCL_BHAIRAV: &str = include_str!("../../assets/scales/bhairav.scl");
const SCL_BHAIRAVI: &str = include_str!("../../assets/scales/bhairavi.scl");
const SCL_YAMAN: &str = include_str!("../../assets/scales/yaman.scl");
const SCL_JOG: &str = include_str!("../../assets/scales/jog.scl");
const SCL_SLENDRO: &str = include_str!("../../assets/scales/slendro.scl");
const SCL_PELOG: &str = include_str!("../../assets/scales/pelog.scl");
const SCL_BOHLEN_PIERCE: &str = include_str!("../../assets/scales/bohlen_pierce.scl");
const SCL_KAFI: &str = include_str!("../../assets/scales/kafi.scl");

/// Registry of named tuning systems.
///
/// Builtins are loaded from embedded .scl files; user scales can be added at runtime.
pub struct TuningRegistry {
    systems: HashMap<String, TuningSystem>,
}

impl TuningRegistry {
    pub fn new() -> Self {
        Self {
            systems: HashMap::new(),
        }
    }

    /// Parse and register all built-in scales.
    pub fn load_builtins(&mut self) {
        let builtins: &[(&str, &str)] = &[
            ("12tet", SCL_12TET),
            ("5limit_ji", SCL_5LIMIT_JI),
            ("bhairav", SCL_BHAIRAV),
            ("bhairavi", SCL_BHAIRAVI),
            ("yaman", SCL_YAMAN),
            ("jog", SCL_JOG),
            ("slendro", SCL_SLENDRO),
            ("pelog", SCL_PELOG),
            ("bohlen_pierce", SCL_BOHLEN_PIERCE),
            ("kafi", SCL_KAFI),
        ];

        for (name, source) in builtins {
            match TuningSystem::from_scl(source) {
                Ok(mut ts) => {
                    ts.name = name.to_string();
                    self.systems.insert(name.to_string(), ts);
                }
                Err(e) => {
                    log::error!("Failed to parse built-in scale '{}': {}", name, e);
                }
            }
        }
    }

    /// Look up a tuning system by name.
    pub fn get(&self, name: &str) -> Option<&TuningSystem> {
        self.systems.get(name)
    }

    /// List all registered tuning names (sorted for deterministic UI order).
    pub fn list(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.systems.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_loads_all_builtins() {
        let mut reg = TuningRegistry::new();
        reg.load_builtins();
        assert_eq!(reg.list().len(), 10);
    }

    #[test]
    fn registry_all_builtins_parse_without_error() {
        let mut reg = TuningRegistry::new();
        reg.load_builtins();
        for name in reg.list() {
            let ts = reg.get(name).unwrap_or_else(|| panic!("missing: {}", name));
            assert!(!ts.cents.is_empty(), "{} has no cents", name);
            assert!(!ts.degrees.is_empty(), "{} has no degrees", name);
        }
    }

    #[test]
    fn registry_get_bhairav() {
        let mut reg = TuningRegistry::new();
        reg.load_builtins();
        let ts = reg.get("bhairav").unwrap();
        assert_eq!(ts.degrees.len(), 7);
    }

    #[test]
    fn registry_bohlen_pierce_period() {
        let mut reg = TuningRegistry::new();
        reg.load_builtins();
        let ts = reg.get("bohlen_pierce").unwrap();
        assert!(
            (ts.period_cents() - 1901.96).abs() < 0.01,
            "BP period should be ~1901.96, got {}",
            ts.period_cents()
        );
    }

    #[test]
    fn registry_list_sorted() {
        let mut reg = TuningRegistry::new();
        reg.load_builtins();
        let names = reg.list();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn registry_get_nonexistent_returns_none() {
        let reg = TuningRegistry::new();
        assert!(reg.get("nonexistent").is_none());
    }
}
