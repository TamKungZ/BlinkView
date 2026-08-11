use crate::media::{self, MediaKind};
use image::{imageops::FilterType, ImageFormat, ImageReader};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn create(input: &Path, output: &Path, size: u32) -> Result<(), String> {
    let size = size.clamp(16, 4096);
    let kind = media::kind_for_path(input)
        .ok_or_else(|| "unsupported media type for thumbnail".to_string())?;

    match kind {
        MediaKind::Image => create_image(input, output, size),
        MediaKind::Video => create_video(input, output, size),
    }
}

fn create_image(input: &Path, output: &Path, size: u32) -> Result<(), String> {
    let decoded = ImageReader::open(input)
        .map_err(|e| format!("thumbnail open failed: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("thumbnail format detection failed: {e}"))?
        .decode()
        .map_err(|e| format!("thumbnail decode failed: {e}"))?;

    // Lanczos is used only for shell thumbnails, not the viewer hot path.
    let thumb = decoded.resize(size, size, FilterType::Lanczos3);
    let file = File::create(output).map_err(|e| format!("thumbnail output failed: {e}"))?;
    let mut writer = BufWriter::new(file);
    thumb
        .write_to(&mut writer, ImageFormat::Png)
        .map_err(|e| format!("thumbnail PNG write failed: {e}"))
}

fn create_video(input: &Path, output: &Path, size: u32) -> Result<(), String> {
    // Try a frame slightly into the clip first; retry from frame zero for very
    // short videos. Force PNG because thumbnailer output paths may lack .png.
    if run_ffmpeg_thumbnail(input, output, size, Some("1.0"))? {
        return Ok(());
    }
    if run_ffmpeg_thumbnail(input, output, size, None)? {
        return Ok(());
    }
    Err("ffmpeg could not extract a video thumbnail".into())
}

fn run_ffmpeg_thumbnail(
    input: &Path,
    output: &Path,
    size: u32,
    seek: Option<&str>,
) -> Result<bool, String> {
    let filter = format!(
        "scale=w={size}:h={size}:force_original_aspect_ratio=decrease"
    );
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-y"]);
    if let Some(seek) = seek {
        cmd.args(["-ss", seek]);
    }
    cmd.arg("-i")
        .arg(input)
        .args(["-frames:v", "1", "-vf"])
        .arg(filter)
        .args(["-an", "-sn", "-dn", "-f", "image2", "-vcodec", "png"])
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_console(&mut cmd);

    let status = cmd
        .status()
        .map_err(|e| format!("ffmpeg is unavailable for video thumbnails: {e}"))?;
    Ok(status.success() && output.is_file())
}

#[cfg(target_os = "windows")]
fn hide_console(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn hide_console(_: &mut Command) {}
