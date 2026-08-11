use crate::cache::ImageCache;
use crate::config::Config;
use crate::media::{self, MediaKind, MediaSet};
use crate::render;
use crate::video::VideoPlayer;
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
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
    let mut mouse_was_down = false;

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
        let mouse_down = window.get_mouse_down(MouseButton::Left);
        if mouse_down && !mouse_was_down {
            if let Some((x, y)) = window.get_mouse_pos(MouseMode::Discard) {
                if let Some(delta) = nav_button_hit(x, y, last_size.0, last_size.1) {
                    navigate += delta;
                }
            }
        }
        mouse_was_down = mouse_down;

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
        let mut framebuffer = match self.current_kind() {
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
        };

        draw_nav_buttons(
            &mut framebuffer,
            width,
            height,
            self.media.index > 0,
            self.media.index + 1 < self.media.items.len(),
        );
        framebuffer
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

#[derive(Clone, Copy)]
enum ButtonSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

impl Rect {
    fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x as f32
            && y >= self.y as f32
            && x < self.x.saturating_add(self.w) as f32
            && y < self.y.saturating_add(self.h) as f32
    }
}

fn nav_button_hit(x: f32, y: f32, width: usize, height: usize) -> Option<isize> {
    if nav_button_rect(width, height, ButtonSide::Left).contains(x, y) {
        return Some(-1);
    }
    if nav_button_rect(width, height, ButtonSide::Right).contains(x, y) {
        return Some(1);
    }
    None
}

fn draw_nav_buttons(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    can_go_left: bool,
    can_go_right: bool,
) {
    if width == 0 || height == 0 || buffer.len() != width.saturating_mul(height) {
        return;
    }
    if !can_go_left && !can_go_right {
        return;
    }

    draw_nav_button(
        buffer,
        width,
        nav_button_rect(width, height, ButtonSide::Left),
        ButtonSide::Left,
        can_go_left,
    );
    draw_nav_button(
        buffer,
        width,
        nav_button_rect(width, height, ButtonSide::Right),
        ButtonSide::Right,
        can_go_right,
    );
}

fn nav_button_rect(width: usize, height: usize, side: ButtonSide) -> Rect {
    let margin = (width / 80).clamp(8, 24).min(width.saturating_sub(1) / 2);
    let available_w = width.saturating_sub(margin.saturating_mul(2)).max(1);
    let available_h = height.saturating_sub(margin.saturating_mul(2)).max(1);
    let w = (width / 16).clamp(44, 72).min(available_w).max(1);
    let h = (height / 6).clamp(72, 128).min(available_h).max(1);
    let y = (height.saturating_sub(h)) / 2;
    let x = match side {
        ButtonSide::Left => margin,
        ButtonSide::Right => width.saturating_sub(margin).saturating_sub(w),
    };
    Rect { x, y, w, h }
}

fn draw_nav_button(buffer: &mut [u32], width: usize, rect: Rect, side: ButtonSide, enabled: bool) {
    let fill_alpha = if enabled { 170 } else { 80 };
    let line_alpha = if enabled { 230 } else { 115 };
    fill_rect(buffer, width, rect, 0x151515, fill_alpha);
    stroke_rect(buffer, width, rect, 0xF2F2F2, line_alpha / 2);

    let thickness = (rect.w.min(rect.h) / 14).clamp(3, 6) as isize;
    let x0 = rect.x as isize;
    let y0 = rect.y as isize;
    let w = rect.w as isize;
    let h = rect.h as isize;
    let top_y = y0 + h * 34 / 100;
    let mid_y = y0 + h / 2;
    let bottom_y = y0 + h * 66 / 100;

    let (tip_x, outer_x) = match side {
        ButtonSide::Left => (x0 + w * 36 / 100, x0 + w * 64 / 100),
        ButtonSide::Right => (x0 + w * 64 / 100, x0 + w * 36 / 100),
    };

    draw_thick_line(
        buffer, width, outer_x, top_y, tip_x, mid_y, thickness, 0xFFFFFF, line_alpha,
    );
    draw_thick_line(
        buffer, width, tip_x, mid_y, outer_x, bottom_y, thickness, 0xFFFFFF, line_alpha,
    );
}

fn fill_rect(buffer: &mut [u32], width: usize, rect: Rect, color: u32, alpha: u32) {
    let height = buffer.len() / width;
    let x_end = rect.x.saturating_add(rect.w).min(width);
    let y_end = rect.y.saturating_add(rect.h).min(height);
    for y in rect.y..y_end {
        for x in rect.x..x_end {
            blend_pixel(buffer, y * width + x, color, alpha);
        }
    }
}

fn stroke_rect(buffer: &mut [u32], width: usize, rect: Rect, color: u32, alpha: u32) {
    if rect.w == 0 || rect.h == 0 {
        return;
    }
    let x2 = rect.x.saturating_add(rect.w).saturating_sub(1);
    let y2 = rect.y.saturating_add(rect.h).saturating_sub(1);
    draw_thick_line(
        buffer,
        width,
        rect.x as isize,
        rect.y as isize,
        x2 as isize,
        rect.y as isize,
        1,
        color,
        alpha,
    );
    draw_thick_line(
        buffer,
        width,
        rect.x as isize,
        y2 as isize,
        x2 as isize,
        y2 as isize,
        1,
        color,
        alpha,
    );
    draw_thick_line(
        buffer,
        width,
        rect.x as isize,
        rect.y as isize,
        rect.x as isize,
        y2 as isize,
        1,
        color,
        alpha,
    );
    draw_thick_line(
        buffer,
        width,
        x2 as isize,
        rect.y as isize,
        x2 as isize,
        y2 as isize,
        1,
        color,
        alpha,
    );
}

fn draw_thick_line(
    buffer: &mut [u32],
    width: usize,
    x1: isize,
    y1: isize,
    x2: isize,
    y2: isize,
    thickness: isize,
    color: u32,
    alpha: u32,
) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let steps = dx.abs().max(dy.abs()).max(1);
    let radius = thickness / 2;

    for step in 0..=steps {
        let x = x1 + dx * step / steps;
        let y = y1 + dy * step / steps;
        for oy in -radius..=radius {
            for ox in -radius..=radius {
                set_pixel(buffer, width, x + ox, y + oy, color, alpha);
            }
        }
    }
}

fn set_pixel(buffer: &mut [u32], width: usize, x: isize, y: isize, color: u32, alpha: u32) {
    if x < 0 || y < 0 || width == 0 {
        return;
    }
    let x = x as usize;
    let y = y as usize;
    let height = buffer.len() / width;
    if x >= width || y >= height {
        return;
    }
    blend_pixel(buffer, y * width + x, color, alpha);
}

fn blend_pixel(buffer: &mut [u32], index: usize, color: u32, alpha: u32) {
    if alpha >= 255 {
        buffer[index] = color;
        return;
    }
    let dst = buffer[index];
    let inv = 255u32.saturating_sub(alpha);
    let r = (((color >> 16) & 0xFF) * alpha + ((dst >> 16) & 0xFF) * inv) / 255;
    let g = (((color >> 8) & 0xFF) * alpha + ((dst >> 8) & 0xFF) * inv) / 255;
    let b = ((color & 0xFF) * alpha + (dst & 0xFF) * inv) / 255;
    buffer[index] = (r << 16) | (g << 8) | b;
}

#[cfg(test)]
mod tests {
    use super::{nav_button_hit, nav_button_rect, ButtonSide, Rect};

    #[test]
    fn nav_buttons_sit_on_opposite_sides() {
        let left = nav_button_rect(1280, 800, ButtonSide::Left);
        let right = nav_button_rect(1280, 800, ButtonSide::Right);

        assert!(left.x < 1280 / 4);
        assert!(right.x > 1280 * 3 / 4);
        assert_eq!(left.y, right.y);
        assert_eq!(left.h, right.h);
    }

    #[test]
    fn nav_hit_detects_buttons() {
        let left = nav_button_rect(320, 240, ButtonSide::Left);
        let right = nav_button_rect(320, 240, ButtonSide::Right);

        assert_eq!(
            nav_button_hit(center_x(left), center_y(left), 320, 240),
            Some(-1)
        );
        assert_eq!(
            nav_button_hit(center_x(right), center_y(right), 320, 240),
            Some(1)
        );
        assert_eq!(nav_button_hit(160.0, 120.0, 320, 240), None);
    }

    fn center_x(rect: Rect) -> f32 {
        (rect.x + rect.w / 2) as f32
    }

    fn center_y(rect: Rect) -> f32 {
        (rect.y + rect.h / 2) as f32
    }
}
