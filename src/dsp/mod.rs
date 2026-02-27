pub mod adsr;
pub mod atom;
pub mod cell;
pub mod command;
pub mod organism_dsp;
pub mod shared;

#[cfg(test)]
mod integration_tests {
    use super::atom::rms;

    const SR: f32 = 44100.0;

    /// Verify DspCommand is Copy and small.
    #[test]
    fn dsp_command_is_copy_and_small() {
        use super::command::DspCommand;
        let cmd = DspCommand::NoteOn {
            freq: 440.0,
            velocity: 1.0,
        };
        let _copy = cmd; // Copy
        let _copy2 = cmd; // Still Copy
        assert!(
            std::mem::size_of::<DspCommand>() <= 16,
            "DspCommand should be <= 16 bytes, got {}",
            std::mem::size_of::<DspCommand>()
        );
    }

    // =========================================================================
    // S12 integration tests: DNA file -> OrganismDsp -> audio verification
    // =========================================================================

    /// Load tblk-alpha DNA -> build OrganismDsp -> send NoteOn -> verify percussive transient.
    #[test]
    fn s12_tblk_dna_produces_percussive_transient() {
        use super::command::DspCommand;
        use super::organism_dsp::OrganismDsp;
        use crate::organism::dna_io;
        use std::path::Path;

        let path = Path::new("assets/dna/tblk-alpha.json");
        let dna = dna_io::load(path).expect("Failed to load tblk-alpha.json");
        assert_eq!(dna.species, "tblk");

        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        // Run for 2 seconds — pattern_gen triggers strike_voice
        let mut buf = Vec::new();
        let mut out = [0.0f32; 2];
        for _ in 0..(SR as usize * 2) {
            org.tick(&mut out);
            buf.push(out[0]);
        }

        let r = rms(&buf);
        assert!(
            r > 0.0001,
            "TBLK organism from DNA should produce percussive audio: rms={r}"
        );
    }

    /// Load dron-alpha DNA -> build OrganismDsp -> tick -> verify continuous audio.
    #[test]
    fn s12_dron_dna_produces_continuous_audio() {
        use super::organism_dsp::OrganismDsp;
        use crate::organism::dna_io;
        use std::path::Path;

        let path = Path::new("assets/dna/dron-alpha.json");
        let dna = dna_io::load(path).expect("Failed to load dron-alpha.json");
        assert_eq!(dna.species, "dron");

        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        // Run for 1 second — drone cells always produce audio
        let mut buf = Vec::new();
        let mut out = [0.0f32; 2];
        for _ in 0..44100 {
            org.tick(&mut out);
            buf.push(out[0]);
        }

        let r = rms(&buf);
        assert!(
            r > 0.01,
            "DRON organism from DNA should produce continuous audio: rms={r}"
        );
    }

    /// Load melo-alpha DNA -> build OrganismDsp -> tick -> verify arpeggiated output.
    #[test]
    fn s12_melo_dna_produces_arpeggiated_output() {
        use super::organism_dsp::OrganismDsp;
        use crate::organism::dna_io;
        use std::path::Path;

        let path = Path::new("assets/dna/melo-alpha.json");
        let dna = dna_io::load(path).expect("Failed to load melo-alpha.json");
        assert_eq!(dna.species, "melo");

        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        // Run for 2 seconds — arpeggiator triggers timbre voice
        let mut buf = Vec::new();
        let mut out = [0.0f32; 2];
        for _ in 0..(SR as usize * 2) {
            org.tick(&mut out);
            buf.push(out[0]);
        }

        let r = rms(&buf);
        assert!(
            r > 0.0001,
            "MELO organism from DNA should produce arpeggiated audio: rms={r}"
        );
    }

    // =========================================================================
    // S13 integration tests: new synth engine presets
    // =========================================================================

    /// Load spiegel.json (SVF drive filter, sine cluster osc) -> verify audio output.
    #[test]
    fn spiegel_dna_loads_and_produces_audio() {
        use super::organism_dsp::OrganismDsp;
        use crate::organism::dna_io;
        use std::path::Path;

        let path = Path::new("assets/dna/spiegel.json");
        let dna = dna_io::load(path).expect("Failed to load spiegel.json");
        assert_eq!(dna.name, "spiegel");
        assert_eq!(dna.species, "melo");

        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        // Run for 2 seconds
        let mut buf = Vec::new();
        let mut out = [0.0f32; 2];
        for _ in 0..(SR as usize * 2) {
            org.tick(&mut out);
            buf.push(out[0]);
        }

        let r = rms(&buf);
        assert!(
            r > 0.0001,
            "Spiegel preset should produce audio: rms={r}"
        );
    }

    /// Load hosono.json (ladder filter, tri cluster osc) -> verify audio output.
    #[test]
    fn hosono_dna_loads_and_produces_audio() {
        use super::organism_dsp::OrganismDsp;
        use crate::organism::dna_io;
        use std::path::Path;

        let path = Path::new("assets/dna/hosono.json");
        let dna = dna_io::load(path).expect("Failed to load hosono.json");
        assert_eq!(dna.name, "hosono");
        assert_eq!(dna.species, "melo");

        let (mut org, _handles) = OrganismDsp::from_dna(&dna, SR).unwrap();

        // Run for 2 seconds
        let mut buf = Vec::new();
        let mut out = [0.0f32; 2];
        for _ in 0..(SR as usize * 2) {
            org.tick(&mut out);
            buf.push(out[0]);
        }

        let r = rms(&buf);
        assert!(
            r > 0.0001,
            "Hosono preset should produce audio: rms={r}"
        );
    }
}
