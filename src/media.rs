use std::cmp::Ordering;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
}

#[derive(Debug, Clone)]
pub struct MediaItem {
    pub path: PathBuf,
    pub kind: MediaKind,
}

#[derive(Debug, Clone)]
pub struct MediaSet {
    pub items: Vec<MediaItem>,
    pub index: usize,
}

pub fn scan_from(input: &Path) -> io::Result<MediaSet> {
    let input = absolutize(input)?;
    let (dir, requested) = if input.is_dir() {
        (input, None)
    } else {
        let parent = input
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "file has no parent folder"))?
            .to_path_buf();
        (parent, Some(input))
    };

    let mut items = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = match entry {
            Ok(v) => v,
            Err(_) => continue,
        };
        let file_type = match entry.file_type() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        if let Some(kind) = kind_for_path(&path) {
            items.push(MediaItem { path, kind });
        }
    }

    items.sort_by(|a, b| natural_path_cmp(&a.path, &b.path));
    if items.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "folder has no supported images or videos",
        ));
    }

    let index = if let Some(requested) = requested {
        items
            .iter()
            .position(|item| same_path(&item.path, &requested))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "requested file is not a supported media file",
                )
            })?
    } else {
        0
    };

    Ok(MediaSet { items, index })
}

pub fn kind_for_path(path: &Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    if matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "ico" | "pnm" | "ppm" | "pgm" | "pbm" | "qoi"
    ) {
        return Some(MediaKind::Image);
    }
    if matches!(
        ext.as_str(),
        "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" | "mpg" | "mpeg" | "wmv" | "flv" | "ts" | "mts" | "m2ts"
    ) {
        return Some(MediaKind::Video);
    }
    None
}

pub fn preload_order(items: &[MediaItem], index: usize, distance: usize) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(distance.saturating_mul(2).saturating_add(1));
    if let Some(item) = items.get(index) {
        if item.kind == MediaKind::Image {
            out.push(item.path.clone());
        }
    }

    for delta in 1..=distance {
        if let Some(item) = items.get(index.saturating_add(delta)) {
            if item.kind == MediaKind::Image {
                out.push(item.path.clone());
            }
        }
        if let Some(left) = index.checked_sub(delta) {
            if let Some(item) = items.get(left) {
                if item.kind == MediaKind::Image {
                    out.push(item.path.clone());
                }
            }
        }
    }
    out
}

pub fn keep_order(items: &[MediaItem], index: usize, radius: usize) -> Vec<PathBuf> {
    preload_order(items, index, radius)
}

fn absolutize(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn natural_path_cmp(a: &Path, b: &Path) -> Ordering {
    let a = a
        .file_name()
        .map(|v| v.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let b = b
        .file_name()
        .map(|v| v.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    natural_cmp(&a, &b)
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let mut ai = 0usize;
    let mut bi = 0usize;

    while ai < ab.len() && bi < bb.len() {
        let ad = ab[ai].is_ascii_digit();
        let bd = bb[bi].is_ascii_digit();

        if ad && bd {
            let a0 = ai;
            let b0 = bi;
            while ai < ab.len() && ab[ai].is_ascii_digit() {
                ai += 1;
            }
            while bi < bb.len() && bb[bi].is_ascii_digit() {
                bi += 1;
            }

            let mut az = a0;
            let mut bz = b0;
            while az < ai && ab[az] == b'0' {
                az += 1;
            }
            while bz < bi && bb[bz] == b'0' {
                bz += 1;
            }
            let alen = ai - az;
            let blen = bi - bz;
            match alen.cmp(&blen) {
                Ordering::Equal => {
                    let ord = ab[az..ai].cmp(&bb[bz..bi]);
                    if ord != Ordering::Equal {
                        return ord;
                    }
                    let ord = (ai - a0).cmp(&(bi - b0));
                    if ord != Ordering::Equal {
                        return ord;
                    }
                }
                ord => return ord,
            }
        } else {
            let ord = ab[ai].cmp(&bb[bi]);
            if ord != Ordering::Equal {
                return ord;
            }
            ai += 1;
            bi += 1;
        }
    }

    ab.len().cmp(&bb.len())
}

#[cfg(test)]
mod tests {
    use super::natural_cmp;
    use std::cmp::Ordering;

    #[test]
    fn natural_numbers_sort_as_humans_expect() {
        assert_eq!(natural_cmp("1.png", "2.png"), Ordering::Less);
        assert_eq!(natural_cmp("2.png", "10.png"), Ordering::Less);
        assert_eq!(natural_cmp("img009.png", "img10.png"), Ordering::Less);
    }
}
