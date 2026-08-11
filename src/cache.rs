use image::ImageReader;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

#[derive(Debug)]
pub struct ImageData {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}

impl ImageData {
    fn byte_len(&self) -> usize {
        self.pixels.len().saturating_mul(std::mem::size_of::<u32>())
    }
}

struct LoadRequest {
    generation: u64,
    path: PathBuf,
}

struct LoadResult {
    generation: u64,
    path: PathBuf,
    result: Result<Arc<ImageData>, String>,
}

pub struct ImageCache {
    map: HashMap<PathBuf, Arc<ImageData>>,
    errors: HashMap<PathBuf, String>,
    queued: HashSet<PathBuf>,
    request_tx: Sender<LoadRequest>,
    result_rx: Receiver<LoadResult>,
    generation: Arc<AtomicU64>,
    current_generation: u64,
    max_bytes: usize,
}

impl ImageCache {
    pub fn new(max_cache_mb: usize) -> Self {
        let (request_tx, request_rx) = mpsc::channel::<LoadRequest>();
        let (result_tx, result_rx) = mpsc::channel::<LoadResult>();
        let generation = Arc::new(AtomicU64::new(1));
        let worker_generation = Arc::clone(&generation);

        thread::Builder::new()
            .name("blinkview-image-loader".into())
            .spawn(move || loader_loop(request_rx, result_tx, worker_generation))
            .expect("failed to spawn image loader thread");

        Self {
            map: HashMap::new(),
            errors: HashMap::new(),
            queued: HashSet::new(),
            request_tx,
            result_rx,
            generation,
            current_generation: 1,
            max_bytes: max_cache_mb.saturating_mul(1024 * 1024),
        }
    }

    pub fn recenter(&mut self, preload_priority: &[PathBuf], keep_priority: &[PathBuf]) {
        self.current_generation = self.current_generation.wrapping_add(1).max(1);
        self.generation
            .store(self.current_generation, Ordering::Release);
        self.queued.clear();

        let keep: HashSet<PathBuf> = keep_priority.iter().cloned().collect();
        self.map.retain(|path, _| keep.contains(path));
        self.errors.retain(|path, _| keep.contains(path));
        self.enforce_budget(keep_priority);

        for path in preload_priority {
            if self.map.contains_key(path) || self.queued.contains(path) {
                continue;
            }
            self.queued.insert(path.clone());
            let _ = self.request_tx.send(LoadRequest {
                generation: self.current_generation,
                path: path.clone(),
            });
        }
    }

    pub fn reset(&mut self) {
        self.current_generation = self.current_generation.wrapping_add(1).max(1);
        self.generation
            .store(self.current_generation, Ordering::Release);
        self.map.clear();
        self.errors.clear();
        self.queued.clear();
    }

    pub fn poll(&mut self, keep_priority: &[PathBuf]) -> bool {
        let keep: HashSet<PathBuf> = keep_priority.iter().cloned().collect();
        let mut changed = false;

        while let Ok(done) = self.result_rx.try_recv() {
            if done.generation != self.current_generation {
                continue;
            }
            self.queued.remove(&done.path);
            if !keep.contains(&done.path) {
                continue;
            }
            match done.result {
                Ok(image) => {
                    self.errors.remove(&done.path);
                    self.map.insert(done.path, image);
                }
                Err(message) => {
                    self.map.remove(&done.path);
                    self.errors.insert(done.path, message);
                }
            }
            changed = true;
        }

        if changed {
            self.enforce_budget(keep_priority);
        }
        changed
    }

    pub fn get(&self, path: &Path) -> Option<Arc<ImageData>> {
        self.map.get(path).cloned()
    }

    pub fn error(&self, path: &Path) -> Option<&str> {
        self.errors.get(path).map(String::as_str)
    }

    pub fn decoded_bytes(&self) -> usize {
        self.map.values().map(|image| image.byte_len()).sum()
    }

    fn enforce_budget(&mut self, keep_priority: &[PathBuf]) {
        if self.max_bytes == 0 {
            return;
        }
        let mut bytes = self.decoded_bytes();
        if bytes <= self.max_bytes {
            return;
        }

        // Keep the first entry (normally current image) even when that single image
        // exceeds the configured budget. Remove farthest neighbours first.
        for path in keep_priority.iter().skip(1).rev() {
            if bytes <= self.max_bytes {
                break;
            }
            if let Some(image) = self.map.remove(path) {
                bytes = bytes.saturating_sub(image.byte_len());
            }
        }
    }
}

fn loader_loop(
    request_rx: Receiver<LoadRequest>,
    result_tx: Sender<LoadResult>,
    generation: Arc<AtomicU64>,
) {
    while let Ok(request) = request_rx.recv() {
        if generation.load(Ordering::Acquire) != request.generation {
            continue;
        }
        let result = decode(&request.path);
        if generation.load(Ordering::Acquire) != request.generation {
            continue;
        }
        if result_tx
            .send(LoadResult {
                generation: request.generation,
                path: request.path,
                result,
            })
            .is_err()
        {
            break;
        }
    }
}

fn decode(path: &Path) -> Result<Arc<ImageData>, String> {
    let image = ImageReader::open(path)
        .map_err(|e| format!("open failed: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("format detection failed: {e}"))?
        .decode()
        .map_err(|e| format!("decode failed: {e}"))?
        .to_rgba8();

    let (width, height) = image.dimensions();
    let raw = image.into_raw();
    let mut pixels = Vec::with_capacity((width as usize).saturating_mul(height as usize));

    for rgba in raw.chunks_exact(4) {
        let a = rgba[3] as u32;
        let r = (rgba[0] as u32 * a + 127) / 255;
        let g = (rgba[1] as u32 * a + 127) / 255;
        let b = (rgba[2] as u32 * a + 127) / 255;
        pixels.push((r << 16) | (g << 8) | b);
    }

    Ok(Arc::new(ImageData {
        width: width as usize,
        height: height as usize,
        pixels,
    }))
}
