use crate::cache::ImageCache;
use crate::config::Config;
use crate::media::{self, MediaKind, MediaSet};
use crate::render;
use crate::video::VideoPlayer;
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

pub enum ViewerOutcome {
    Hidden,
    Quit,
}

pub fn run(
    initial_path: PathBuf,
    cfg: &Config,
    ipc_rx: &Receiver<PathBuf>,
) -> Result<ViewerOutcome, String> {
    let media_set = media::scan_from(&initial_path).map_err(|e| e.to_string())?;
    let mut state = ViewerState::new(media_set, cfg)?;

    let mut window = Window::new(
        "BlinkView",
        1280,
        800,
        WindowOptions {
            resize: true,
            ..WindowOptions::default()
        },
    )
    .map_err(|e| format!("could not create window: {e}"))?;
    window.set_target_fps(60);
    window.set_background_color(0, 0, 0);
    window.set_cursor_visibility(true);

    let mut framebuffer = vec![0u32; 1280 * 800];
    let mut last_size = (1280usize, 800usize);
    let mut dirty = true;

    while window.is_open() {
        while let Ok(path) = ipc_rx.try_recv() {
            if let Ok(media_set) = media::scan_from(&path) {
                state.replace_media_set(media_set, cfg)?;
                dirty = true;
            }
        }

        let keep = media::keep_order(&state.media.items, state.media.index, cfg.cache_radius);
        if state.cache.poll(&keep) {
            dirty = true;
        }

        if let Some(video) = state.video.as_mut() {
            if video.poll_frame() {
                dirty = true;
            }
        }

        if window.is_key_pressed(Key::Q, KeyRepeat::No) {
            return Ok(ViewerOutcome::Quit);
        }
        if window.is_key_pressed(Key::Escape, KeyRepeat::No) {
            return Ok(ViewerOutcome::Hidden);
        }

        let mut navigate = 0isize;
        if window.is_key_pressed(Key::Right, KeyRepeat::Yes) {
            navigate += 1;
        }
        if window.is_key_pressed(Key::Left, KeyRepeat::Yes) {
            navigate -= 1;
        }
        if window.is_key_pressed(Key::PageDown, KeyRepeat::Yes) {
            navigate += 10;
        }
        if window.is_key_pressed(Key::PageUp, KeyRepeat::Yes) {
            navigate -= 10;
        }

        if navigate != 0 {
            state.navigate(navigate, cfg)?;
            dirty = true;
        }
        if window.is_key_pressed(Key::Home, KeyRepeat::No) {
            state.set_index(0, cfg)?;
            dirty = true;
        }
        if window.is_key_pressed(Key::End, KeyRepeat::No) {
            let last = state.media.items.len().saturating_sub(1);
            state.set_index(last, cfg)?;
            dirty = true;
        }
        if window.is_key_pressed(Key::Space, KeyRepeat::No) {
            if let Some(video) = state.video.as_mut() {
                video.toggle_pause();
                dirty = true;
            }
        }
        if window.is_key_pressed(Key::R, KeyRepeat::No) {
            let current = state.current_path().to_path_buf();
            if let Ok(media_set) = media::scan_from(&current) {
                state.replace_media_set(media_set, cfg)?;
                dirty = true;
            }
        }

        let size = window.get_size();
        let size = (size.0.max(1), size.1.max(1));
        if size != last_size {
            last_size = size;
            dirty = true;
        }

        if dirty {
            framebuffer = state.render(last_size.0, last_size.1);
            window.set_title(&state.title());
            dirty = false;
        }

        window
            .update_with_buffer(&framebuffer, last_size.0, last_size.1)
            .map_err(|e| format!("window update failed: {e}"))?;
    }

    Ok(ViewerOutcome::Hidden)
}

struct ViewerState {
    media: MediaSet,
    cache: ImageCache,
    video: Option<VideoPlayer>,
    video_error: Option<String>,
}

impl ViewerState {
    fn new(media: MediaSet, cfg: &Config) -> Result<Self, String> {
        let mut state = Self {
            media,
            cache: ImageCache::new(cfg.max_cache_mb),
            video: None,
            video_error: None,
        };
        state.open_current(cfg)?;
        Ok(state)
    }

    fn replace_media_set(&mut self, media: MediaSet, cfg: &Config) -> Result<(), String> {
        self.media = media;
        self.cache.reset();
        self.open_current(cfg)
    }

    fn navigate(&mut self, delta: isize, cfg: &Config) -> Result<(), String> {
        if self.media.items.is_empty() {
            return Ok(());
        }
        let last = self.media.items.len() as isize - 1;
        let next = (self.media.index as isize + delta).clamp(0, last) as usize;
        self.set_index(next, cfg)
    }

    fn set_index(&mut self, index: usize, cfg: &Config) -> Result<(), String> {
        if index == self.media.index || index >= self.media.items.len() {
            return Ok(());
        }
        self.media.index = index;
        self.open_current(cfg)
    }

    fn open_current(&mut self, cfg: &Config) -> Result<(), String> {
        self.video = None;
        self.video_error = None;

        let preload = media::preload_order(&self.media.items, self.media.index, cfg.preload);
        let keep = media::keep_order(&self.media.items, self.media.index, cfg.cache_radius);
        self.cache.recenter(&preload, &keep);

        if self.current_kind() == MediaKind::Video {
            match VideoPlayer::open(self.current_path()) {
                Ok(video) => self.video = Some(video),
                Err(err) => self.video_error = Some(err),
            }
        }
        Ok(())
    }

    fn current_kind(&self) -> MediaKind {
        self.media.items[self.media.index].kind
    }

    fn current_path(&self) -> &Path {
        &self.media.items[self.media.index].path
    }

    fn render(&self, width: usize, height: usize) -> Vec<u32> {
        match self.current_kind() {
            MediaKind::Image => {
                if let Some(image) = self.cache.get(self.current_path()) {
                    render::fit_into(&image.pixels, image.width, image.height, width, height)
                } else {
                    vec![0u32; width.saturating_mul(height)]
                }
            }
            MediaKind::Video => {
                if let Some(video) = &self.video {
                    if let Some(frame) = video.current_frame() {
                        render::fit_into(frame, video.width, video.height, width, height)
                    } else {
                        vec![0u32; width.saturating_mul(height)]
                    }
                } else {
                    vec![0u32; width.saturating_mul(height)]
                }
            }
        }
    }

    fn title(&self) -> String {
        let filename = self
            .current_path()
            .file_name()
            .map(|v| v.to_string_lossy())
            .unwrap_or_else(|| self.current_path().to_string_lossy());
        let position = format!("{}/{}", self.media.index + 1, self.media.items.len());

        match self.current_kind() {
            MediaKind::Image => {
                if let Some(err) = self.cache.error(self.current_path()) {
                    format!("{filename} [{position}] - image error: {err} - BlinkView")
                } else if self.cache.get(self.current_path()).is_some() {
                    let mb = self.cache.decoded_bytes() as f64 / 1024.0 / 1024.0;
                    format!("{filename} [{position}] - cache {mb:.0} MB - BlinkView")
                } else {
                    format!("{filename} [{position}] - loading - BlinkView")
                }
            }
            MediaKind::Video => {
                if let Some(err) = &self.video_error {
                    format!("{filename} [{position}] - video error: {err} - BlinkView")
                } else if let Some(video) = &self.video {
                    let mode = if video.paused() {
                        "paused"
                    } else if video.ended() {
                        "ended"
                    } else {
                        "playing"
                    };
                    format!(
                        "{filename} [{position}] - {mode} {:.2} fps - BlinkView",
                        video.fps
                    )
                } else {
                    format!("{filename} [{position}] - loading video - BlinkView")
                }
            }
        }
    }
}
