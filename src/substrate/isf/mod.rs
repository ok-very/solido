//! Interactive Shader Format (ISF) parser.
//!
//! ISF wraps GLSL/WGSL shaders with a JSON header describing typed input
//! parameters, render passes, and metadata. This module parses that header
//! and converts ISF inputs to Module port schemas — the key bridge that
//! lets every ISF shader become a live-patchable visual Module.
//!
//! ISF spec reference: https://isf.video/spec/
//!
//! The JSON header sits inside a `/* { ... } */` comment at the top of the
//! shader source file.

mod parser;
mod types;
