use crossbeam_channel::{bounded, Receiver, Sender};
use egui::{Context, TextureHandle, TextureOptions};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder};
use lru::LruCache;
use std::collections::{hash_map::DefaultHasher, HashSet};
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

pub const THUMB_MAX_PX: u32 = 512;
const CACHE_CAPACITY: usize = 350;

struct ThumbRequest {
    path: PathBuf,
    max_px: u32,
}

struct ThumbResult {
    path: PathBuf,
    pixels: Option<Vec<u8>>,
    size: [u32; 2],
}

pub struct ThumbnailStore {
    request_tx: Sender<ThumbRequest>,
    result_rx: Receiver<ThumbResult>,
    in_flight: HashSet<PathBuf>,
    failed: HashSet<PathBuf>,
    cache: LruCache<PathBuf, CachedTexture>,
}

pub struct CachedTexture {
    pub texture: TextureHandle,
    pub size: [u32; 2],
}

impl ThumbnailStore {
    pub fn new() -> Self {
        let (request_tx, request_rx) = bounded::<ThumbRequest>(512);
        let (result_tx, result_rx) = crossbeam_channel::unbounded::<ThumbResult>();

        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(2, 8);

        for index in 0..workers {
            let request_rx = request_rx.clone();
            let result_tx = result_tx.clone();
            std::thread::Builder::new()
                .name(format!("gallery-thumb-{index}"))
                .spawn(move || {
                    while let Ok(request) = request_rx.recv() {
                        let (pixels, size) = match load_thumbnail(&request.path, request.max_px) {
                            Ok((pixels, size)) => (Some(pixels), size),
                            Err(err) => {
                                log::warn!(
                                    "thumbnail failed for {}: {err}",
                                    request.path.display()
                                );
                                (None, [0, 0])
                            }
                        };
                        let _ = result_tx.send(ThumbResult {
                            path: request.path,
                            pixels,
                            size,
                        });
                    }
                })
                .ok();
        }

        Self {
            request_tx,
            result_rx,
            in_flight: HashSet::new(),
            failed: HashSet::new(),
            cache: LruCache::new(NonZeroUsize::new(CACHE_CAPACITY).unwrap()),
        }
    }

    pub fn get(&mut self, path: &Path) -> Option<&CachedTexture> {
        self.cache.get(path)
    }

    pub fn busy(&self) -> bool {
        !self.in_flight.is_empty()
    }

    pub fn request(&mut self, path: &Path) {
        if self.cache.contains(path) || self.in_flight.contains(path) || self.failed.contains(path)
        {
            return;
        }
        let request = ThumbRequest {
            path: path.to_path_buf(),
            max_px: THUMB_MAX_PX,
        };
        if self.request_tx.try_send(request).is_ok() {
            self.in_flight.insert(path.to_path_buf());
        }
    }

    pub fn poll(&mut self, ctx: &Context) {
        while let Ok(result) = self.result_rx.try_recv() {
            self.in_flight.remove(&result.path);
            match result.pixels {
                Some(pixels) => {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [result.size[0] as usize, result.size[1] as usize],
                        &pixels,
                    );
                    let texture = ctx.load_texture(
                        result.path.display().to_string(),
                        image,
                        TextureOptions::LINEAR,
                    );
                    self.cache.put(
                        result.path,
                        CachedTexture {
                            texture,
                            size: result.size,
                        },
                    );
                }
                None => {
                    self.failed.insert(result.path);
                }
            }
        }
    }
}

fn load_thumbnail(path: &Path, max_px: u32) -> Result<(Vec<u8>, [u32; 2]), String> {
    let cache_path = cache_file_for(path);
    if let Some(cache_path) = &cache_path {
        if let Ok(cached) = image::open(cache_path) {
            return Ok((
                cached.to_rgba8().into_raw(),
                [cached.width(), cached.height()],
            ));
        }
    }

    let image = decode_image(path)?;
    let thumb = image.thumbnail(max_px, max_px);
    let rgba = thumb.to_rgba8();
    let size = [rgba.width(), rgba.height()];

    if let Some(cache_path) = cache_path {
        save_cache(&thumb, &cache_path);
    }

    Ok((rgba.into_raw(), size))
}

fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("rust-gallery").join("thumbs"))
}

fn cache_file_for(path: &Path) -> Option<PathBuf> {
    let dir = cache_dir()?;
    let metadata = std::fs::metadata(path).ok()?;
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    if let Ok(modified) = metadata.modified() {
        if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
            duration.as_nanos().hash(&mut hasher);
        }
    }
    let key = hasher.finish();
    Some(dir.join(format!("{key:016x}.jpg")))
}

fn save_cache(image: &DynamicImage, path: &Path) {
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let tmp = path.with_extension("jpg.tmp");
    let Ok(file) = std::fs::File::create(&tmp) else {
        return;
    };
    let mut writer = std::io::BufWriter::new(file);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 84);
    if encoder.encode_image(image).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

pub fn decode_image(path: &Path) -> Result<DynamicImage, String> {
    let reader = image::ImageReader::open(path)
        .map_err(|err| format!("open: {err}"))?
        .with_guessed_format()
        .map_err(|err| format!("format: {err}"))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|err| format!("decoder: {err}"))?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder).map_err(|err| format!("decode: {err}"))?;
    image.apply_orientation(orientation);
    Ok(image)
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
