#![allow(dead_code)]
use super::types::*;

/// Parse an ISF shader source string into an IsfShader.
///
/// The ISF format embeds a JSON header inside `/* { ... } */` at the top
/// of the shader source. This parser extracts that JSON, parses it with
/// serde_json, and returns the structured result along with the remaining
/// shader source code.
pub fn parse_isf(source: &str) -> Result<IsfShader, IsfParseError> {
    // Find the JSON header: scan /* ... */ blocks until one parses as a JSON object.
    let mut json_val = None;
    let mut _header_abs_start = 0;
    let mut header_abs_end = 0;

    let mut search_idx = 0;
    while let Some(start) = source[search_idx..].find("/*") {
        let abs_start = search_idx + start;
        if let Some(end) = source[abs_start..].find("*/") {
            let json_str = source[abs_start + 2..abs_start + end].trim();
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                if parsed.is_object() {
                    json_val = Some(parsed);
                    _header_abs_start = abs_start;
                    header_abs_end = abs_start + end;
                    break;
                }
            }
            search_idx = abs_start + end + 2;
        } else {
            break; // Unclosed comment, stop searching
        }
    }

    let json = json_val.ok_or(IsfParseError::NoHeader)?;

    let name = json
        .get("DESCRIPTION")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled ISF")
        .to_string();

    let description = json
        .get("CREDIT")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Parse INPUTS array
    let inputs = if let Some(arr) = json.get("INPUTS").and_then(|v| v.as_array()) {
        arr.iter()
            .filter_map(|input| parse_input(input))
            .collect()
    } else {
        Vec::new()
    };

    // Parse PASSES array
    let passes = if let Some(arr) = json.get("PASSES").and_then(|v| v.as_array()) {
        arr.iter().map(|pass| parse_pass(pass)).collect()
    } else {
        Vec::new()
    };

    // Shader source is everything after the header comment
    let shader_source = source[header_abs_end + 2..].trim().to_string();

    Ok(IsfShader {
        name,
        description,
        inputs,
        passes,
        source: shader_source,
    })
}

fn parse_input(json: &serde_json::Value) -> Option<IsfInput> {
    let name = json.get("NAME")?.as_str()?.to_string();
    let type_str = json.get("TYPE")?.as_str()?;
    let input_type = IsfInputType::from_str(type_str)?;

    Some(IsfInput {
        name,
        input_type,
        label: json.get("LABEL").and_then(|v| v.as_str()).map(String::from),
        default: json.get("DEFAULT").and_then(|v| v.as_f64()),
        min: json.get("MIN").and_then(|v| v.as_f64()),
        max: json.get("MAX").and_then(|v| v.as_f64()),
        values: json
            .get("VALUES")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect()),
        labels: json
            .get("LABELS")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            }),
    })
}

fn parse_pass(json: &serde_json::Value) -> IsfPass {
    IsfPass {
        target: json.get("TARGET").and_then(|v| v.as_str()).map(String::from),
        width: json.get("WIDTH").and_then(|v| v.as_str()).map(String::from),
        height: json.get("HEIGHT").and_then(|v| v.as_str()).map(String::from),
        persistent: json
            .get("PERSISTENT")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        float: json
            .get("FLOAT")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

#[derive(Debug)]
pub enum IsfParseError {
    NoHeader,
    UnclosedHeader,
    InvalidJson(String),
}

impl std::fmt::Display for IsfParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoHeader => write!(f, "no ISF header found (expected /* ... */)"),
            Self::UnclosedHeader => write!(f, "unclosed ISF header comment"),
            Self::InvalidJson(e) => write!(f, "invalid ISF JSON: {e}"),
        }
    }
}

impl std::error::Error for IsfParseError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::signal::SignalType;

    const TEST_ISF: &str = r#"/*
{
    "DESCRIPTION": "Thermal Burst",
    "CREDIT": "A test thermal shader",
    "INPUTS": [
        {
            "NAME": "brightness",
            "TYPE": "float",
            "LABEL": "Brightness",
            "DEFAULT": 0.5,
            "MIN": 0.0,
            "MAX": 1.0
        },
        {
            "NAME": "trigger",
            "TYPE": "event"
        },
        {
            "NAME": "inputImage",
            "TYPE": "image"
        },
        {
            "NAME": "color_seed",
            "TYPE": "color"
        }
    ],
    "PASSES": [
        {
            "TARGET": "buffer0",
            "PERSISTENT": true
        }
    ]
}
*/

void main() {
    gl_FragColor = vec4(1.0);
}
"#;

    #[test]
    fn parse_test_shader() {
        let shader = parse_isf(TEST_ISF).unwrap();
        assert_eq!(shader.name, "Thermal Burst");
        assert_eq!(shader.description, "A test thermal shader");
        assert_eq!(shader.inputs.len(), 4);
        assert_eq!(shader.passes.len(), 1);
        assert!(shader.source.contains("void main()"));
    }

    #[test]
    fn isf_input_types() {
        let shader = parse_isf(TEST_ISF).unwrap();
        assert_eq!(shader.inputs[0].input_type, IsfInputType::Float);
        assert_eq!(shader.inputs[0].default, Some(0.5));
        assert_eq!(shader.inputs[0].min, Some(0.0));
        assert_eq!(shader.inputs[0].max, Some(1.0));
        assert_eq!(shader.inputs[1].input_type, IsfInputType::Event);
        assert_eq!(shader.inputs[2].input_type, IsfInputType::Image);
        assert_eq!(shader.inputs[3].input_type, IsfInputType::Color);
    }

    #[test]
    fn isf_pass_config() {
        let shader = parse_isf(TEST_ISF).unwrap();
        let pass = &shader.passes[0];
        assert_eq!(pass.target, Some("buffer0".to_string()));
        assert!(pass.persistent);
        assert!(!pass.float);
    }

    #[test]
    fn to_module_schema_conversion() {
        let shader = parse_isf(TEST_ISF).unwrap();
        let schema = shader.to_module_schema();

        assert_eq!(schema.name, "Thermal Burst");
        assert_eq!(schema.inputs.len(), 4);
        assert_eq!(schema.outputs.len(), 1);

        // brightness → Float input
        let brightness = schema.input("brightness").unwrap();
        assert_eq!(brightness.signal_type, SignalType::Float);
        assert_eq!(brightness.range, Some((0.0, 1.0)));

        // trigger → Trigger input
        let trigger = schema.input("trigger").unwrap();
        assert_eq!(trigger.signal_type, SignalType::Trigger);

        // inputImage → FrameRef input
        let img = schema.input("inputImage").unwrap();
        assert_eq!(img.signal_type, SignalType::FrameRef);

        // color_seed → PixelSample input
        let color = schema.input("color_seed").unwrap();
        assert_eq!(color.signal_type, SignalType::PixelSample);

        // Output: rendered_frame → FrameRef
        let out = schema.output("rendered_frame").unwrap();
        assert_eq!(out.signal_type, SignalType::FrameRef);

        assert!(schema.side_effects.contains(&"gpu_render".to_string()));
    }

    #[test]
    fn no_header_fails() {
        let result = parse_isf("void main() {}");
        assert!(result.is_err());
    }
}
