use std::path::PathBuf;

fn main() {
    // Copy FFmpeg DLLs to the output directory so the binary can find them at runtime.
    let ffmpeg_dir = match std::env::var("FFMPEG_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => return, // No FFmpeg, skip
    };

    let dll_dir = ffmpeg_dir.join("bin");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    // OUT_DIR is target/debug/build/<pkg>/out — walk up to target/debug or target/release
    let target_dir = out_dir
        .ancestors()
        .find(|p| p.ends_with("debug") || p.ends_with("release"))
        .map(|p| p.to_path_buf());

    let Some(target) = target_dir else { return };

    if let Ok(entries) = std::fs::read_dir(&dll_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "dll") {
                let dest = target.join(path.file_name().unwrap());
                if !dest.exists() {
                    let _ = std::fs::copy(&path, &dest);
                }
            }
        }
    }
}
