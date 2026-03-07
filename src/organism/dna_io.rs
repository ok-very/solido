use std::path::Path;

use super::dna::OrganismDna;

/// Save organism DNA to a JSON file.
pub fn save(dna: &OrganismDna, path: &Path) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(dna)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

/// Load organism DNA from a JSON file.
pub fn load(path: &Path) -> std::io::Result<OrganismDna> {
    let json = std::fs::read_to_string(path)?;
    serde_json::from_str(&json)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organism::dna::*;
    use std::collections::BTreeMap;

    fn make_test_dna() -> OrganismDna {
        let mut params = BTreeMap::new();
        params.insert("membrane_freq".into(), 180.0);
        params.insert("bandwidth".into(), 60.0);
        OrganismDna {
            name: "io-test".into(),
            species: "tblk".into(),
            active: true,
            seed: 12345,
            version: 1,
            cells: vec![CellDna {
                cell_type: "hexo_strike".into(),
                params,
                string_params: BTreeMap::new(),
            }],
            cell_wiring: vec![CellWire {
                src_cell: 0,
                dst_cell: 0,
                wire_type: WireType::Audio,
                gain: 1.0,
                mode: WireMode::default(),
            }],
            body: BodyDna::default(),
            render: RenderDna::default(),
            physics: PhysicsDna::default(),
            emotion: EmotionDna::default(),
            sends: None,
            interaction_params: None,
            affinity_tags: vec!["test".into()],
            affinity_biases: vec![],
            fidelity: 0.5,
            scale_affinity: 0.5,
            rhythm_affinity: 0.5,
            rhythm_sync: "none".into(),
        }
    }

    #[test]
    fn roundtrip_save_load_produces_identical_json() {
        let dna = make_test_dna();
        let dir = std::env::temp_dir().join("solido_dna_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path1 = dir.join("roundtrip1.json");
        let path2 = dir.join("roundtrip2.json");

        save(&dna, &path1).unwrap();
        let loaded = load(&path1).unwrap();
        save(&loaded, &path2).unwrap();

        let json1 = std::fs::read_to_string(&path1).unwrap();
        let json2 = std::fs::read_to_string(&path2).unwrap();
        assert_eq!(json1, json2, "Round-trip save->load->save should produce identical JSON");

        // Cleanup
        let _ = std::fs::remove_file(&path1);
        let _ = std::fs::remove_file(&path2);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn load_nonexistent_returns_error() {
        let result = load(Path::new("/nonexistent/path/dna.json"));
        assert!(result.is_err());
    }
}
