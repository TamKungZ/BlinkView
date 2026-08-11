use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

pub struct VideoPlayer {
    pub width: usize,
    pub height: usize,
    pub fps: f64,
    path: PathBuf,
    receiver: Receiver<Vec<u32>>,
    recycle: SyncSender<Vec<u32>>,
    current: Option<Vec<u32>>,
    next_due: Instant,
    frame_duration: Duration,
    presented_frames: u64,
    paused: bool,
    ended: bool,
    audio: Option<Child>,
    audio_started: bool,
}

impl VideoPlayer {
    pub fn open(path: &Path) -> Result<Self, String> {
        let (width, height, fps) = probe(path)?;
        let frame_pixels = width
            .checked_mul(height)
            .ok_or_else(|| "video frame dimensions are too large".to_string())?;

        let (frame_tx, frame_rx) = mpsc::sync_channel::<Vec<u32>>(2);
        let (recycle_tx, recycle_rx) = mpsc::sync_channel::<Vec<u32>>(3);
        let decode_path = path.to_path_buf();

        thread::Builder::new()
            .name("blinkview-video-decoder".into())
            .spawn(move || decode_loop(&decode_path, frame_pixels, frame_tx, recycle_rx))
            .map_err(|e| format!("could not start video decoder thread: {e}"))?;

        let fps = if fps.is_finite() && fps > 0.1 { fps } else { 30.0 };
        let frame_duration = Duration::from_secs_f64(1.0 / fps.clamp(1.0, 240.0));

        Ok(Self {
            width,
            height,
            fps,
            path: path.to_path_buf(),
            receiver: frame_rx,
            recycle: recycle_tx,
            current: None,
            next_due: Instant::now(),
            frame_duration,
            presented_frames: 0,
            paused: false,
            ended: false,
            audio: None,
            audio_started: false,
        })
    }

    pub fn toggle_pause(&mut self) {
        if self.ended {
            return;
        }

        self.paused = !self.paused;
        self.next_due = Instant::now() + self.frame_duration;

        if self.paused {
            self.stop_audio();
        } else if self.current.is_some() {
            self.start_audio(self.playback_seconds());
        }
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn ended(&self) -> bool {
        self.ended
    }

    pub fn current_frame(&self) -> Option<&[u32]> {
        self.current.as_deref()
    }

    pub fn poll_frame(&mut self) -> bool {
        let now = Instant::now();
        if self.paused || now < self.next_due {
            return false;
        }

        // Start the presentation clock (and audio) only when the first video
        // frame actually arrives. Decoder startup therefore does not put audio
        // ahead of the picture on a cold open.
        if self.current.is_none() {
            return match self.receiver.try_recv() {
                Ok(frame) => {
                    self.current = Some(frame);
                    self.presented_frames = 0;
                    self.next_due = now + self.frame_duration;
                    if !self.audio_started {
                        self.audio_started = true;
                        self.start_audio(0.0);
                    }
                    true
                }
                Err(TryRecvError::Empty) => false,
                Err(TryRecvError::Disconnected) => {
                    let changed = !self.ended;
                    self.ended = true;
                    self.stop_audio();
                    changed
                }
            };
        }

        // If rendering stalls, consume enough queued frames to catch the media
        // clock up instead of permanently slowing playback. At normal speed this
        // is exactly one frame. This also lets >60 fps sources keep their duration
        // when the UI itself is polling at 60 Hz by dropping presentation frames.
        let late = now.saturating_duration_since(self.next_due);
        let extra_due = (late.as_secs_f64() / self.frame_duration.as_secs_f64()) as usize;
        let frames_due = 1usize.saturating_add(extra_due).min(16);

        let mut newest = None;
        let mut consumed = 0usize;
        let mut status_changed = false;

        for _ in 0..frames_due {
            match self.receiver.try_recv() {
                Ok(frame) => {
                    consumed += 1;
                    if let Some(skipped) = newest.replace(frame) {
                        let _ = self.recycle.try_send(skipped);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    status_changed = !self.ended;
                    self.ended = true;
                    self.stop_audio();
                    break;
                }
            }
        }

        if let Some(frame) = newest {
            if let Some(old) = self.current.replace(frame) {
                let _ = self.recycle.try_send(old);
            }
            self.presented_frames = self.presented_frames.saturating_add(consumed as u64);
            self.next_due += self.frame_duration.mul_f64(consumed as f64);
            if now.saturating_duration_since(self.next_due) > Duration::from_secs(1) {
                self.next_due = now + self.frame_duration;
            }
            true
        } else {
            status_changed
        }
    }

    fn playback_seconds(&self) -> f64 {
        self.presented_frames as f64 / self.fps.max(0.1)
    }

    fn start_audio(&mut self, offset_seconds: f64) {
        self.stop_audio();

        let offset = format!("{:.6}", offset_seconds.max(0.0));
        let mut cmd = Command::new("ffplay");
        cmd.args([
            "-hide_banner",
            "-loglevel",
            "quiet",
            "-nodisp",
            "-autoexit",
            "-vn",
            "-ss",
        ])
        .arg(offset)
        .arg(&self.path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        hide_console(&mut cmd);

        // Audio is intentionally best-effort. Some minimal FFmpeg packages do
        // not ship ffplay, and a video may not contain an audio stream. Neither
        // case should prevent BlinkView from displaying the video.
        self.audio = cmd.spawn().ok();
    }

    fn stop_audio(&mut self) {
        if let Some(mut child) = self.audio.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        self.stop_audio();
    }
}

fn probe(path: &Path) -> Result<(usize, usize, f64), String> {
    let mut cmd = Command::new("ffprobe");
    cmd.args([
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=width,height,avg_frame_rate",
        "-of",
        "default=noprint_wrappers=1",
    ])
    .arg(path)
    .stdin(Stdio::null())
    .stderr(Stdio::piped());
    hide_console(&mut cmd);

    let output = cmd
        .output()
        .map_err(|e| format!("ffprobe is unavailable: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut width = None;
    let mut height = None;
    let mut fps = None;

    for line in text.lines() {
        if let Some(value) = line.strip_prefix("width=") {
            width = value.parse::<usize>().ok();
        } else if let Some(value) = line.strip_prefix("height=") {
            height = value.parse::<usize>().ok();
        } else if let Some(value) = line.strip_prefix("avg_frame_rate=") {
            fps = parse_rate(value);
        }
    }

    let width = width.ok_or_else(|| "ffprobe did not return video width".to_string())?;
    let height = height.ok_or_else(|| "ffprobe did not return video height".to_string())?;
    Ok((width, height, fps.unwrap_or(30.0)))
}

fn parse_rate(value: &str) -> Option<f64> {
    if let Some((n, d)) = value.split_once('/') {
        let n = n.parse::<f64>().ok()?;
        let d = d.parse::<f64>().ok()?;
        if d.abs() < f64::EPSILON {
            return None;
        }
        Some(n / d)
    } else {
        value.parse::<f64>().ok()
    }
}

fn decode_loop(
    path: &Path,
    frame_pixels: usize,
    frame_tx: SyncSender<Vec<u32>>,
    recycle_rx: Receiver<Vec<u32>>,
) {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-loglevel", "error", "-i"])
        .arg(path)
        .args(["-an", "-sn", "-dn", "-f", "rawvideo", "-pix_fmt", "bgra", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_console(&mut cmd);

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(_) => return,
    };
    let mut stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => return,
    };

    loop {
        let mut frame = match recycle_rx.try_recv() {
            Ok(mut recycled) if recycled.len() == frame_pixels => {
                recycled.fill(0);
                recycled
            }
            _ => vec![0u32; frame_pixels],
        };

        let bytes = as_bytes_mut(&mut frame);
        if let Err(err) = stdout.read_exact(bytes) {
            if err.kind() != io::ErrorKind::UnexpectedEof {
                // Decoder error; just terminate the stream.
            }
            break;
        }

        normalize_bgra(&mut frame);
        if frame_tx.send(frame).is_err() {
            break;
        }
    }

    let _ = child.kill();
    let _ = child.wait();
}

fn as_bytes_mut(words: &mut [u32]) -> &mut [u8] {
    // u32 has no invalid bit patterns, and the returned slice has exactly the
    // same lifetime and backing allocation as `words`.
    unsafe {
        std::slice::from_raw_parts_mut(
            words.as_mut_ptr().cast::<u8>(),
            words.len().saturating_mul(std::mem::size_of::<u32>()),
        )
    }
}

fn normalize_bgra(frame: &mut [u32]) {
    #[cfg(target_endian = "little")]
    {
        for pixel in frame {
            *pixel &= 0x00FF_FFFF;
        }
    }

    #[cfg(target_endian = "big")]
    {
        for pixel in frame {
            let raw = pixel.to_be_bytes();
            let b = raw[0] as u32;
            let g = raw[1] as u32;
            let r = raw[2] as u32;
            *pixel = (r << 16) | (g << 8) | b;
        }
    }
}

#[cfg(target_os = "windows")]
fn hide_console(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn hide_console(_: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::parse_rate;

    #[test]
    fn parses_fractional_fps() {
        let fps = parse_rate("30000/1001").unwrap();
        assert!((fps - 29.970).abs() < 0.01);
    }
}
