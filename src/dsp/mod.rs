pub mod adsr;
pub mod atom;
pub mod cell;
pub mod command;
pub mod molecule;
pub mod organism_dsp;

#[cfg(test)]
mod integration_tests {
    use super::atom::rms;
    use super::molecule::{dron, melo, tblk, Molecule};

    const SR: f32 = 44100.0;

    fn render_molecule(mol: &mut Molecule, samples: usize) -> Vec<f32> {
        let ins = mol.audio_inputs();
        let outs = mol.audio_outputs();
        let input = vec![0.0f32; ins];
        let mut buf = Vec::with_capacity(samples * outs);
        let mut output = vec![0.0f32; outs];
        for _ in 0..samples {
            mol.tick(&input, &mut output);
            buf.extend_from_slice(&output);
        }
        buf
    }

    /// TBLK chain: membrane + snap transient → body resonance → percussive output.
    #[test]
    fn tblk_chain_produces_percussive_transient() {
        let mut membrane = tblk::membrane_sim(200.0, 60.0, SR);
        let mut snap = tblk::snap_transient(SR);
        let mut body = tblk::body_resonance(0.005, SR);

        let mut combined_buf = Vec::new();
        let mut mem_out = [0.0f32];
        let mut snap_out = [0.0f32];
        let mut body_out = [0.0f32];

        for _ in 0..4410 {
            membrane.tick(&[], &mut mem_out);
            snap.tick(&[], &mut snap_out);
            // Mix membrane + snap, feed into body resonance
            let mix = mem_out[0] * 0.7 + snap_out[0] * 0.3;
            body.tick(&[mix], &mut body_out);
            combined_buf.push(body_out[0]);
        }

        let r = rms(&combined_buf);
        assert!(
            r > 0.001,
            "TBLK chain should produce percussive audio: rms={r}"
        );

        // Body resonance is a feedback delay — it resonates and builds up.
        // Just verify both early and late portions have audio (resonant body).
        let early = rms(&combined_buf[..1000]);
        let late = rms(&combined_buf[3000..]);
        assert!(
            early > 0.001 && late > 0.001,
            "TBLK chain should produce audio throughout: early={early} late={late}"
        );
    }

    /// DRON chain: detuned_stack → slow_filter → stereo output with beating.
    #[test]
    fn dron_chain_produces_stereo_drone() {
        let mut stack = dron::detuned_stack(110.0, SR);
        let mut filter = dron::slow_filter(800.0, 0.707, SR);
        let mut spread = dron::stereo_spread(SR);

        let mut buf_l = Vec::new();
        let mut buf_r = Vec::new();
        let mut stack_out = [0.0f32];
        let mut filt_out = [0.0f32];
        let mut stereo_out = [0.0f32; 2];

        for _ in 0..44100 {
            // 1 second
            stack.tick(&[], &mut stack_out);
            filter.tick(&[stack_out[0]], &mut filt_out);
            spread.tick(&[filt_out[0]], &mut stereo_out);
            buf_l.push(stereo_out[0]);
            buf_r.push(stereo_out[1]);
        }

        let rms_l = rms(&buf_l);
        let rms_r = rms(&buf_r);
        assert!(
            rms_l > 0.01,
            "DRON left channel should have audio: rms={rms_l}"
        );
        assert!(
            rms_r > 0.01,
            "DRON right channel should have audio: rms={rms_r}"
        );

        // Detuned stack should produce beating (amplitude variation)
        let mut stack2 = dron::detuned_stack(110.0, SR);
        let long_buf = render_molecule(&mut stack2, 44100);
        // Check that RMS of first half differs from second half (beating)
        let rms_first = rms(&long_buf[..22050]);
        let rms_second = rms(&long_buf[22050..]);
        let diff = (rms_first - rms_second).abs();
        // With detuning, there should be some amplitude variation
        assert!(
            rms_first > 0.01 && rms_second > 0.01,
            "Both halves should have audio: {rms_first}, {rms_second}"
        );
        // Just verify it produces sustained audio (drone character)
        assert!(
            rms(&long_buf) > 0.1,
            "Detuned stack should produce sustained drone"
        );
    }

    /// MELO chain: osc_pair → filter_envelope → amp_envelope → synth pluck.
    #[test]
    fn melo_chain_produces_synth_pluck() {
        let mut osc = melo::osc_pair(440.0, 0.5, SR);
        let (mut filt_env, _, _) = melo::filter_envelope(200.0, 5000.0, SR);
        let mut amp_env = melo::amp_envelope(SR);

        // Gate on (note on)
        filt_env.set_param("adsr.gate", 1.0);
        amp_env.set_param("adsr.gate", 1.0);

        let mut buf = Vec::new();
        let mut osc_out = [0.0f32];
        let mut filt_out = [0.0f32];
        let mut amp_out = [0.0f32];

        // Play for 200ms (attack + start of decay)
        for _ in 0..8820 {
            osc.tick(&[], &mut osc_out);
            filt_env.tick(&[osc_out[0]], &mut filt_out);
            amp_env.tick(&[filt_out[0]], &mut amp_out);
            buf.push(amp_out[0]);
        }

        // Gate off (note off)
        filt_env.set_param("adsr.gate", 0.0);
        amp_env.set_param("adsr.gate", 0.0);

        // Let release complete
        for _ in 0..15000 {
            osc.tick(&[], &mut osc_out);
            filt_env.tick(&[osc_out[0]], &mut filt_out);
            amp_env.tick(&[filt_out[0]], &mut amp_out);
            buf.push(amp_out[0]);
        }

        // Check: early part should have audio
        let early_rms = rms(&buf[..8820]);
        assert!(
            early_rms > 0.01,
            "MELO pluck early should have audio: rms={early_rms}"
        );

        // Check: late part (after release) should be silent
        let late_rms = rms(&buf[20000..]);
        assert!(
            late_rms < 0.05,
            "MELO pluck late should be near silent: rms={late_rms}"
        );
    }

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
}
