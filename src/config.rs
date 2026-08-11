use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub background: bool,
    pub preload: usize,
    pub cache_radius: usize,
    pub max_cache_mb: usize,
    pub port: u16,
    pub initial_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            background: false,
            preload: 2,
            cache_radius: 4,
            max_cache_mb: 384,
            port: 43119,
            initial_path: None,
        }
    }
}

pub enum ParseResult {
    Run(Config),
    Thumbnail {
        input: PathBuf,
        output: PathBuf,
        size: u32,
    },
    Startup(bool),
    Help,
}

impl Config {
    pub fn parse() -> Result<ParseResult, String> {
        let raw: Vec<_> = env::args_os().skip(1).collect();

        if raw.first().is_some_and(|v| v.to_string_lossy() == "--thumbnail") {
            if raw.len() < 3 || raw.len() > 4 {
                return Err("--thumbnail needs INPUT OUTPUT [SIZE]".into());
            }
            let size = if let Some(value) = raw.get(3) {
                value
                    .to_string_lossy()
                    .parse::<u32>()
                    .map_err(|_| "thumbnail SIZE must be a positive integer".to_string())?
            } else {
                256
            };
            if size == 0 {
                return Err("thumbnail SIZE must be greater than zero".into());
            }
            return Ok(ParseResult::Thumbnail {
                input: PathBuf::from(raw[1].clone()),
                output: PathBuf::from(raw[2].clone()),
                size,
            });
        }

        if raw.first().is_some_and(|v| v.to_string_lossy() == "--startup") {
            if raw.len() != 2 {
                return Err("--startup needs enable or disable".into());
            }
            let enabled = match raw[1].to_string_lossy().to_ascii_lowercase().as_str() {
                "enable" | "on" | "1" => true,
                "disable" | "off" | "0" => false,
                _ => return Err("--startup needs enable or disable".into()),
            };
            return Ok(ParseResult::Startup(enabled));
        }

        let mut cfg = Config::default();
        let mut args = raw.into_iter().peekable();
        let mut positional_only = false;

        while let Some(arg) = args.next() {
            if positional_only {
                if cfg.initial_path.is_none() {
                    cfg.initial_path = Some(PathBuf::from(arg));
                } else {
                    return Err("only one path may be opened at a time".into());
                }
                continue;
            }

            let text = arg.to_string_lossy();
            match text.as_ref() {
                "--" => positional_only = true,
                "-h" | "--help" => return Ok(ParseResult::Help),
                "-b" | "--background" | "--stay" => cfg.background = true,
                "--preload" => {
                    cfg.preload = parse_usize(args.next(), "--preload")?;
                }
                "--cache-radius" => {
                    cfg.cache_radius = parse_usize(args.next(), "--cache-radius")?;
                }
                "--max-cache-mb" => {
                    cfg.max_cache_mb = parse_usize(args.next(), "--max-cache-mb")?;
                }
                "--port" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--port needs a value".to_string())?;
                    cfg.port = value
                        .to_string_lossy()
                        .parse::<u16>()
                        .map_err(|_| "--port must be 1..65535".to_string())?;
                    if cfg.port == 0 {
                        return Err("--port must be 1..65535".into());
                    }
                }
                _ if text.starts_with('-') => {
                    return Err(format!("unknown option: {text}"));
                }
                _ => {
                    if cfg.initial_path.is_none() {
                        cfg.initial_path = Some(PathBuf::from(arg));
                    } else {
                        return Err("only one path may be opened at a time".into());
                    }
                }
            }
        }

        cfg.cache_radius = cfg.cache_radius.max(cfg.preload);
        cfg.max_cache_mb = cfg.max_cache_mb.max(32);
        Ok(ParseResult::Run(cfg))
    }
}

fn parse_usize(value: Option<std::ffi::OsString>, flag: &str) -> Result<usize, String> {
    let value = value.ok_or_else(|| format!("{flag} needs a value"))?;
    value
        .to_string_lossy()
        .parse::<usize>()
        .map_err(|_| format!("{flag} needs a non-negative integer"))
}

pub fn help_text() -> &'static str {
    r#"BlinkView 0.1.1 - small folder-aware image/video viewer
Author: TamKungZ_ <dev@tamkungz.me>

USAGE:
    blinkview [OPTIONS] <FILE-OR-DIRECTORY>
    blinkview --background
    blinkview --thumbnail INPUT OUTPUT [SIZE]
    blinkview --startup enable|disable

OPTIONS:
    -b, --background        Stay resident after the viewer window closes.
        --stay              Alias for --background.
        --preload N         Decode N neighbours on each side-ish (default: 2).
        --cache-radius N    Keep only current +/- N folder items decoded (default: 4).
        --max-cache-mb N    Hard-ish decoded image cache budget (default: 384 MB).
        --port N            Local single-instance IPC TCP port (default: 43119).
        --thumbnail ...     Write a PNG thumbnail. Video thumbnails use FFmpeg.
        --startup ...       Enable/disable login startup in background mode.
    -h, --help              Show help.

KEYS:
    Left / Right            Previous / next media file.
    PageUp / PageDown       Jump 10 items.
    Home / End              First / last item.
    Space                   Pause/resume video and audio.
    R                       Reload current folder/file.
    Esc                     Close/hide viewer window.
    Q                       Quit the process, even in background mode.

VIDEO:
    Video frames use ffprobe + ffmpeg. Lightweight audio uses ffplay when available.
    If ffplay is missing, video continues silently instead of failing.
"#
}
