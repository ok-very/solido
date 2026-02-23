use std::any::Any;

use crate::module::port::{Port, PortRate};
use crate::module::schema::{ModuleCategory, ModuleSchema};
use crate::module::signal::{Signal, SignalType};
use crate::module::{ModuleCore, PortId, SignalError};

/// Cursor input module — emits normalized X/Y position every tick.
///
/// Position is normalized to [0, 1] by the caller before feeding.
pub struct CursorInputModule {
    schema: ModuleSchema,
    x: f32,
    y: f32,
    pixel: [f32; 4],
    x_port: PortId,
    y_port: PortId,
    pixel_port: PortId,
}

impl CursorInputModule {
    pub fn new() -> Self {
        let x_out = Port::output("x", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Normalized cursor X position");
        let y_out = Port::output("y", SignalType::Float, PortRate::Block)
            .with_range(0.0, 1.0)
            .with_description("Normalized cursor Y position");
        let pixel_out = Port::output("pixel", SignalType::PixelSample, PortRate::Block)
            .with_description("RGBA at cursor position from SDF framebuffer");

        let x_port = x_out.id;
        let y_port = y_out.id;
        let pixel_port = pixel_out.id;

        let schema = ModuleSchema::new("cursor_input", ModuleCategory::Input)
            .with_description("Emits cursor position and pixel sample each tick")
            .with_output(x_out)
            .with_output(y_out)
            .with_output(pixel_out);

        Self {
            schema,
            x: 0.0,
            y: 0.0,
            pixel: [0.0; 4],
            x_port,
            y_port,
            pixel_port,
        }
    }

    /// Update cursor position. Values should be normalized to [0, 1].
    pub fn feed_position(&mut self, x: f32, y: f32) {
        self.x = x.clamp(0.0, 1.0);
        self.y = y.clamp(0.0, 1.0);
    }

    /// Feed RGBA pixel sample at the cursor position.
    /// Called from app layer when GPU readback is available.
    pub fn feed_pixel(&mut self, rgba: [f32; 4]) {
        self.pixel = rgba;
    }

}

impl ModuleCore for CursorInputModule {
    fn schema(&self) -> &ModuleSchema {
        &self.schema
    }

    fn emit_signals(&mut self, buffer: &mut Vec<(PortId, Signal)>) {
        buffer.push((self.x_port, Signal::Float(self.x)));
        buffer.push((self.y_port, Signal::Float(self.y)));
        buffer.push((self.pixel_port, Signal::PixelSample(self.pixel)));
    }

    fn receive_signal(&mut self, port: PortId, _signal: Signal) -> Result<(), SignalError> {
        Err(SignalError::UnknownPort(port))
    }

    fn tick(&mut self, _dt: f32) {
        // Position persists until next feed_position call
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_position_and_pixel_every_tick() {
        let mut module = CursorInputModule::new();
        module.feed_position(0.3, 0.7);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer[0].0, module.x_port);
        assert_eq!(buffer[1].0, module.y_port);
        assert_eq!(buffer[2].0, module.pixel_port);

        if let Signal::Float(x) = &buffer[0].1 {
            assert!((x - 0.3).abs() < 1e-5);
        } else {
            panic!("expected Float for x");
        }
        if let Signal::Float(y) = &buffer[1].1 {
            assert!((y - 0.7).abs() < 1e-5);
        } else {
            panic!("expected Float for y");
        }
        assert!(matches!(buffer[2].1, Signal::PixelSample(_)));
    }

    #[test]
    fn clamps_out_of_range_values() {
        let mut module = CursorInputModule::new();
        module.feed_position(-0.5, 1.5);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        if let Signal::Float(x) = &buffer[0].1 {
            assert_eq!(*x, 0.0);
        }
        if let Signal::Float(y) = &buffer[1].1 {
            assert_eq!(*y, 1.0);
        }
    }

    #[test]
    fn position_persists_across_ticks() {
        let mut module = CursorInputModule::new();
        module.feed_position(0.5, 0.5);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);
        assert_eq!(buffer.len(), 3);

        // Second tick without feed_position
        buffer.clear();
        module.tick(1.0 / 60.0);
        module.emit_signals(&mut buffer);
        assert_eq!(buffer.len(), 3);
        if let Signal::Float(x) = &buffer[0].1 {
            assert!((x - 0.5).abs() < 1e-5, "position should persist");
        }
    }

    #[test]
    fn default_position_is_zero() {
        let mut module = CursorInputModule::new();
        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        if let Signal::Float(x) = &buffer[0].1 {
            assert_eq!(*x, 0.0);
        }
        if let Signal::Float(y) = &buffer[1].1 {
            assert_eq!(*y, 0.0);
        }
    }

    #[test]
    fn pixel_sample_fed_and_emitted() {
        let mut module = CursorInputModule::new();
        module.feed_pixel([1.0, 0.5, 0.25, 1.0]);

        let mut buffer = Vec::new();
        module.emit_signals(&mut buffer);

        if let Signal::PixelSample(rgba) = &buffer[2].1 {
            assert_eq!(*rgba, [1.0, 0.5, 0.25, 1.0]);
        } else {
            panic!("expected PixelSample");
        }
    }

    #[test]
    fn receive_signal_returns_unknown_port() {
        let mut module = CursorInputModule::new();
        let result = module.receive_signal(PortId(999), Signal::Float(1.0));
        assert!(matches!(result, Err(SignalError::UnknownPort(_))));
    }

    #[test]
    fn schema_has_correct_ports() {
        let module = CursorInputModule::new();
        let schema = module.schema();
        assert_eq!(schema.name, "cursor_input");
        assert_eq!(schema.category, ModuleCategory::Input);
        assert_eq!(schema.outputs.len(), 3);
        assert_eq!(schema.inputs.len(), 0);
        assert!(schema.output("x").is_some());
        assert!(schema.output("y").is_some());
        assert!(schema.output("pixel").is_some());
    }
}
