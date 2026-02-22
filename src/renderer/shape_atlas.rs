use std::io::Cursor;

pub struct ShapeAtlas {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl ShapeAtlas {
    pub fn load_png(png_bytes: &[u8]) -> Self {
        let decoder = png::Decoder::new(Cursor::new(png_bytes));
        let mut reader = decoder.read_info().expect("Failed to read shape PNG info");
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let frame_info = reader.next_frame(&mut buf).expect("Failed to decode shape PNG");

        let width = frame_info.width;
        let height = frame_info.height;

        let pixels = match frame_info.color_type {
            png::ColorType::Rgba => buf[..frame_info.buffer_size()].to_vec(),
            png::ColorType::Rgb => {
                let rgb = &buf[..frame_info.buffer_size()];
                let pixel_count = (width * height) as usize;
                let mut rgba = Vec::with_capacity(pixel_count * 4);
                for i in 0..pixel_count {
                    rgba.push(rgb[i * 3]);
                    rgba.push(rgb[i * 3 + 1]);
                    rgba.push(rgb[i * 3 + 2]);
                    rgba.push(255);
                }
                rgba
            }
            other => panic!("Unsupported shape PNG color type: {:?}", other),
        };

        Self {
            pixels,
            width,
            height,
        }
    }
}
