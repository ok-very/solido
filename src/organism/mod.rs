// Organism simulation: soft bodies with pseudopods, interaction physics, fusion.
//
// S09 creates: sim, interaction, registry
// S12 creates: dna, dna_io, mutation
// S13 creates: module (OrganismModule bridge)

pub mod dna;
pub mod dna_io;
pub mod interaction;
pub mod module;
pub mod mutation;
pub mod registry;
pub mod sim;
pub mod sonar;
