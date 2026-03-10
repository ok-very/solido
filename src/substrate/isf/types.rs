#![allow(dead_code)]
use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema};
use crate::module::signal::SignalType;

/// A parsed ISF shader with its metadata and source code.
#[derive(Debug, Clone)]
pub struct IsfShader {
    pub name: String,
    pub description: String,
    pub inputs: Vec<IsfInput>,
    pub passes: Vec<IsfPass>,
    /// The shader source code (without the JSON header comment).
    pub source: String,
}

/// An input parameter declared in the ISF JSON header.
#[derive(Debug, Clone)]
pub struct IsfInput {
    pub name: String,
    pub input_type: IsfInputType,
    pub label: Option<String>,
    pub default: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// For long-type inputs.
    pub values: Option<Vec<i64>>,
    pub labels: Option<Vec<String>>,
}

/// ISF input parameter types.
#[derive(Debug, Clone, PartialEq)]
pub enum IsfInputType {
    Float,
    Bool,
    Long,
    Event,
    Point2D,
    Color,
    Image,
}

/// A render pass in the ISF pipeline.
#[derive(Debug, Clone)]
pub struct IsfPass {
    pub target: Option<String>,
    pub width: Option<String>,
    pub height: Option<String>,
    pub persistent: bool,
    pub float: bool,
}

impl IsfShader {
    /// Convert this ISF shader's parameters into a ModuleSchema.
    ///
    /// Each ISF input becomes a Module input port. The module also gets
    /// a single FrameRef output port for the rendered result.
    pub fn to_module_schema(&self) -> ModuleSchema {
        let mut schema = ModuleSchema::new(&self.name, ModuleCategory::Visual)
            .with_description(&self.description);

        for input in &self.inputs {
            let (signal_type, rate) = match input.input_type {
                IsfInputType::Float => (SignalType::Float, PortRate::Block),
                IsfInputType::Bool => (SignalType::Bool, PortRate::Event),
                IsfInputType::Long => (SignalType::Float, PortRate::Event),
                IsfInputType::Event => (SignalType::Trigger, PortRate::Event),
                IsfInputType::Point2D => (SignalType::Embedding, PortRate::Block),
                IsfInputType::Color => (SignalType::PixelSample, PortRate::Block),
                IsfInputType::Image => (SignalType::FrameRef, PortRate::Block),
            };

            let mut port = Port::input(&input.name, signal_type, rate);
            if let Some(label) = &input.label {
                port = port.with_description(label);
            }
            if let (Some(min), Some(max)) = (input.min, input.max) {
                port = port.with_range(min as f32, max as f32);
            }

            schema = schema.with_input(port);
        }

        // Every ISF visual module outputs a rendered frame
        schema = schema.with_output(Port::output(
            "rendered_frame",
            SignalType::FrameRef,
            PortRate::Block,
        ));

        schema = schema.with_side_effect("gpu_render");

        schema
    }
}

impl IsfInputType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "float" => Some(Self::Float),
            "bool" => Some(Self::Bool),
            "long" => Some(Self::Long),
            "event" => Some(Self::Event),
            "point2D" => Some(Self::Point2D),
            "color" => Some(Self::Color),
            "image" => Some(Self::Image),
            _ => None,
        }
    }
}
