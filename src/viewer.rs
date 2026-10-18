use crate::nvjpeg::{is_jpeg, NvJpegDecoder};
use crate::thumbs::decode_image;
use crate::zerocopy::{Downsampler, ZeroCopyAllocation, ZeroCopyConverter};
use crossbeam_channel::{unbounded, Receiver, Sender};
use egui::TextureId;
use egui_wgpu::RenderState;
use lru::LruCache;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use wgpu::{Device, Queue};

const CACHE_BYTES: u64 = 1_200 * 1024 * 1024;
const CACHE_ENTRIES: usize = 32;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CacheKey {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}

impl CacheKey {
    pub fn new(path: PathBuf, target: [u32; 2]) -> Self {
        Self {
            path,
            width: target[0],
            height: target[1],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Timing {
    pub decode: Duration,
    pub upload: Duration,
    pub total: Duration,
}

struct LoadRequest {
    key: CacheKey,
}

struct Slot {
    request: Mutex<Option<LoadRequest>>,
    ready: Condvar,
}

impl Slot {
    fn new() -> Self {
        Self {
            request: Mutex::new(None),
            ready: Condvar::new(),
        }
    }

    fn submit(&self, request: LoadRequest) -> Option<LoadRequest> {
        let old = if let Ok(mut slot) = self.request.lock() {
            let old = slot.replace(request);
            self.ready.notify_one();
            old
        } else {
            None
        };
        old
    }

    fn wait(&self, stop: &AtomicBool) -> Option<LoadRequest> {
        let mut slot = self.request.lock().ok()?;
        loop {
            if stop.load(Ordering::Relaxed) {
                return None;
            }
            if let Some(request) = slot.take() {
                return Some(request);
            }
            slot = self.ready.wait(slot).ok()?;
        }
    }
}

struct UploadedImage {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    source: [u32; 2],
    timing: Timing,
    zero_copy: Option<ZeroCopyAllocation>,
}

struct DecodedImage {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    source: [u32; 2],
    decode: Duration,
    upload: Duration,
    zero_copy: Option<ZeroCopyAllocation>,
}

struct LoadResult {
    key: CacheKey,
    result: Result<UploadedImage, String>,
}

pub struct CachedImage {
    id: TextureId,
    view: wgpu::TextureView,
    _texture: wgpu::Texture,
    _zero_copy: Option<ZeroCopyAllocation>,
    pub width: u32,
    pub height: u32,
    bytes: u64,
    render_state: Arc<RenderState>,
}

impl Drop for CachedImage {
    fn drop(&mut self) {
        if self._zero_copy.is_some() {
            let _ = self.render_state.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            });
        }
        self.render_state.renderer.write().free_texture(&self.id);
    }
}

pub struct ViewerService {
    render_state: Arc<RenderState>,
    current_slot: Arc<Slot>,
    prefetch_slot: Arc<Slot>,
    stop: Arc<AtomicBool>,
    result_rx: Receiver<LoadResult>,
    cache: LruCache<CacheKey, CachedImage>,
    in_flight: HashSet<CacheKey>,
    dimensions: HashMap<PathBuf, [u32; 2]>,
    total_bytes: u64,
    target: Option<CacheKey>,
    pub loading: bool,
    pub error: Option<String>,
    pub last_timing: Option<Timing>,
}

impl ViewerService {
    pub fn new(render_state: &RenderState) -> Self {
        let render_state = Arc::new(render_state.clone());
        let current_slot = Arc::new(Slot::new());
        let prefetch_slot = Arc::new(Slot::new());
        let stop = Arc::new(AtomicBool::new(false));
        let (result_tx, result_rx) = unbounded();

        spawn_worker(
            current_slot.clone(),
            stop.clone(),
            result_tx.clone(),
            render_state.device.clone(),
            render_state.queue.clone(),
            "gallery-viewer",
        );
        spawn_worker(
            prefetch_slot.clone(),
            stop.clone(),
            result_tx,
            render_state.device.clone(),
            render_state.queue.clone(),
            "gallery-prefetch",
        );

        Self {
            render_state,
            current_slot,
            prefetch_slot,
            stop,
            result_rx,
            cache: LruCache::new(NonZeroUsize::new(CACHE_ENTRIES).unwrap()),
            in_flight: HashSet::new(),
            dimensions: HashMap::new(),
            total_bytes: 0,
            target: None,
            loading: false,
            error: None,
            last_timing: None,
        }
    }

    pub fn set_target(&mut self, path: &Path, target: [u32; 2]) {
        let key = CacheKey::new(path.to_path_buf(), target);
        if self.target.as_ref() == Some(&key) {
            return;
        }

        self.target = Some(key.clone());
        self.error = None;

        if self.texture_id(path, target).is_some() {
            self.loading = false;
            return;
        }

        self.loading = true;
        if self.in_flight.insert(key.clone()) {
            if let Some(old) = self.current_slot.submit(LoadRequest { key }) {
                self.in_flight.remove(&old.key);
            }
        }
    }

    pub fn prefetch(&mut self, path: &Path, target: [u32; 2]) {
        let key = CacheKey::new(path.to_path_buf(), target);
        if self.texture_id(path, target).is_some() || self.in_flight.contains(&key) {
            return;
        }
        if self.in_flight.insert(key.clone()) {
            if let Some(old) = self.prefetch_slot.submit(LoadRequest { key }) {
                self.in_flight.remove(&old.key);
            }
        }
    }

    pub fn texture_id(&mut self, path: &Path, target: [u32; 2]) -> Option<TextureId> {
        let key = CacheKey::new(path.to_path_buf(), target);
        if let Some(image) = self.cache.get(&key) {
            return Some(image.id);
        }

        self.cache
            .iter()
            .find(|(candidate, image)| {
                candidate.path == path && image.width >= target[0] && image.height >= target[1]
            })
            .map(|(_, image)| image.id)
    }

    /// The cached full-image texture, for callers that need the underlying
    /// wgpu objects (the grade pass). Mirrors [`ViewerService::texture_id`].
    pub fn base_texture(
        &mut self,
        path: &Path,
        target: [u32; 2],
    ) -> Option<(TextureId, wgpu::TextureView, [u32; 2])> {
        let key = CacheKey::new(path.to_path_buf(), target);
        if let Some(image) = self.cache.get(&key) {
            return Some((image.id, image.view.clone(), [image.width, image.height]));
        }
        self.cache
            .iter()
            .find(|(candidate, image)| {
                candidate.path == path && image.width >= target[0] && image.height >= target[1]
            })
            .map(|(_, image)| (image.id, image.view.clone(), [image.width, image.height]))
    }

    pub fn source_dimensions(&self, path: &Path) -> Option<[u32; 2]> {
        self.dimensions.get(path).copied()
    }

    pub fn deactivate(&mut self) {
        self.target = None;
        self.loading = false;
        self.error = None;
    }

    pub fn poll(&mut self) {
        while let Ok(result) = self.result_rx.try_recv() {
            self.in_flight.remove(&result.key);

            let is_active = self
                .target
                .as_ref()
                .is_some_and(|target| target == &result.key);
            if is_active {
                self.loading = false;
            }

            match result.result {
                Ok(uploaded) => {
                    if self.cache.contains(&result.key) {
                        continue;
                    }
                    let id = self.render_state.renderer.write().register_native_texture(
                        &self.render_state.device,
                        &uploaded.view,
                        wgpu::FilterMode::Linear,
                    );
                    let bytes = uploaded.width as u64 * uploaded.height as u64 * 4;
                    let image = CachedImage {
                        id,
                        view: uploaded.view,
                        _texture: uploaded.texture,
                        _zero_copy: uploaded.zero_copy,
                        width: uploaded.width,
                        height: uploaded.height,
                        bytes,
                        render_state: self.render_state.clone(),
                    };
                    self.total_bytes = self.total_bytes.saturating_add(bytes);
                    self.dimensions
                        .insert(result.key.path.clone(), uploaded.source);
                    if let Some(old) = self.cache.put(result.key.clone(), image) {
                        self.total_bytes = self.total_bytes.saturating_sub(old.bytes);
                    }
                    if is_active {
                        self.last_timing = Some(uploaded.timing);
                        log::info!(
                            "viewer load {}x{} decode {:.1} ms, upload {:.1} ms, total {:.1} ms",
                            uploaded.width,
                            uploaded.height,
                            uploaded.timing.decode.as_secs_f64() * 1000.0,
                            uploaded.timing.upload.as_secs_f64() * 1000.0,
                            uploaded.timing.total.as_secs_f64() * 1000.0
                        );
                    }
                    self.evict();
                }
                Err(error) => {
                    if is_active {
                        self.error = Some(error);
                    }
                }
            }
        }
    }

    fn evict(&mut self) {
        while self.total_bytes > CACHE_BYTES && self.cache.len() > 1 {
            let Some((key, image)) = self.cache.pop_lru() else {
                break;
            };
            if self.target.as_ref() == Some(&key) {
                self.cache.put(key, image);
                break;
            }
            self.total_bytes = self.total_bytes.saturating_sub(image.bytes);
        }
    }
}

impl Drop for ViewerService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.current_slot.ready.notify_all();
        self.prefetch_slot.ready.notify_all();
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_worker(
    slot: Arc<Slot>,
    stop: Arc<AtomicBool>,
    result_tx: Sender<LoadResult>,
    device: Device,
    queue: Queue,
    name: &str,
) {
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let mut worker = DecodeWorker::new(device.clone(), queue.clone());
            while let Some(request) = slot.wait(&stop) {
                let started = Instant::now();
                let result = decode_and_upload(&mut worker, &request.key, &device, &queue);
                let total = started.elapsed();
                let result = result.map(|decoded| fit_to_display(&worker, decoded, &request.key));
                match result {
                    Ok(decoded) => {
                        let _ = result_tx.send(LoadResult {
                            key: request.key,
                            result: Ok(UploadedImage {
                                texture: decoded.texture,
                                view: decoded.view,
                                width: decoded.width,
                                height: decoded.height,
                                source: decoded.source,
                                timing: Timing {
                                    decode: decoded.decode,
                                    upload: decoded.upload,
                                    total,
                                },
                                zero_copy: decoded.zero_copy,
                            }),
                        });
                    }
                    Err(error) => {
                        let _ = result_tx.send(LoadResult {
                            key: request.key,
                            result: Err(error),
                        });
                    }
                }
            }
        })
        .ok();
}

struct DecodeWorker {
    nvjpeg: Option<NvJpegDecoder>,
    nvjpeg_ready: bool,
    zero_copy: Option<ZeroCopyConverter>,
    downsampler: Downsampler,
}

impl DecodeWorker {
    fn new(device: Device, queue: Queue) -> Self {
        let downsampler = Downsampler::new(device.clone(), queue.clone());
        let zero_copy = if zero_copy_enabled() {
            match ZeroCopyConverter::new(device, queue) {
                Ok(converter) => {
                    log::info!("zero-copy converter initialised");
                    Some(converter)
                }
                Err(error) => {
                    log::warn!("zero-copy unavailable: {error}");
                    None
                }
            }
        } else {
            None
        };
        let mut worker = Self {
            nvjpeg: None,
            nvjpeg_ready: false,
            zero_copy,
            downsampler,
        };
        worker.ensure_nvjpeg();
        worker
    }

    fn ensure_nvjpeg(&mut self) {
        if self.nvjpeg_ready {
            return;
        }
        self.nvjpeg_ready = true;
        match NvJpegDecoder::new() {
            Ok(decoder) => {
                log::info!("nvJPEG decoder initialised");
                self.nvjpeg = Some(decoder);
            }
            Err(error) => log::warn!("nvJPEG unavailable, using CPU decode: {error}"),
        }
    }

    fn zero_copy(
        &mut self,
        path: &Path,
        target: [u32; 2],
    ) -> Option<Result<crate::zerocopy::ZeroCopyUpload, String>> {
        if !zero_copy_enabled() || !is_jpeg(path) || !has_normal_orientation(path) {
            return None;
        }
        self.ensure_nvjpeg();
        let decoder = self.nvjpeg.as_mut()?;
        let converter = self.zero_copy.as_ref()?;
        Some(converter.decode(decoder, path, target))
    }

    fn rgba(
        &mut self,
        path: &Path,
        target: [u32; 2],
    ) -> Result<(Vec<u8>, u32, u32, [u32; 2]), String> {
        self.ensure_nvjpeg();

        if is_jpeg(path) && has_normal_orientation(path) {
            if let Some(decoder) = self.nvjpeg.as_mut() {
                match std::fs::read(path) {
                    Ok(data) => {
                        let started = Instant::now();
                        match decoder.decode_jpeg(&data, target) {
                            Ok((rgba, [width, height], source)) => {
                                log::info!(
                                    "nvJPEG {} -> {}x{} (source {}x{}) in {:.1} ms",
                                    path.display(),
                                    width,
                                    height,
                                    source[0],
                                    source[1],
                                    started.elapsed().as_secs_f64() * 1000.0
                                );
                                return Ok((rgba, width, height, source));
                            }
                            Err(error) => {
                                log::warn!("nvJPEG decode failed for {}: {error}", path.display());
                            }
                        }
                    }
                    Err(error) => log::warn!("read failed for {}: {error}", path.display()),
                }
            }
        }

        decode_cpu(path, target)
    }
}

fn decode_cpu(path: &Path, target: [u32; 2]) -> Result<(Vec<u8>, u32, u32, [u32; 2]), String> {
    let image = decode_image(path)?;
    let source = [image.width(), image.height()];
    let image = image.thumbnail(target[0].max(1), target[1].max(1));
    let rgba = image.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    Ok((rgba.into_raw(), width, height, source))
}

fn has_normal_orientation(path: &Path) -> bool {
    use image::metadata::Orientation;
    use image::ImageDecoder;
    let Ok(reader) = image::ImageReader::open(path) else {
        return false;
    };
    let Ok(reader) = reader.with_guessed_format() else {
        return false;
    };
    let Ok(mut decoder) = reader.into_decoder() else {
        return false;
    };
    matches!(
        decoder.orientation().unwrap_or(Orientation::NoTransforms),
        Orientation::NoTransforms
    )
}

fn decode_and_upload(
    worker: &mut DecodeWorker,
    key: &CacheKey,
    device: &Device,
    queue: &Queue,
) -> Result<DecodedImage, String> {
    if let Some(result) = worker.zero_copy(&key.path, [key.width, key.height]) {
        match result {
            Ok(upload) => {
                log::info!(
                    "zero-copy {}x{} source {}x{} decode {:.1} ms, convert {:.1} ms",
                    upload.width,
                    upload.height,
                    upload.source[0],
                    upload.source[1],
                    upload.decode_time.as_secs_f64() * 1000.0,
                    upload.convert_time.as_secs_f64() * 1000.0
                );
                return Ok(DecodedImage {
                    texture: upload.texture,
                    view: upload.view,
                    width: upload.width,
                    height: upload.height,
                    source: upload.source,
                    decode: upload.decode_time,
                    upload: upload.convert_time,
                    zero_copy: Some(upload.allocation),
                });
            }
            Err(error) => log::warn!(
                "zero-copy failed for {}, falling back: {error}",
                key.path.display()
            ),
        }
    }

    let decode_started = Instant::now();
    let (rgba, width, height, source) = worker.rgba(&key.path, [key.width, key.height])?;
    let decode = decode_started.elapsed();

    let upload_started = Instant::now();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("viewer-image"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let upload = upload_started.elapsed();
    Ok(DecodedImage {
        texture,
        view,
        width,
        height,
        source,
        decode,
        upload,
        zero_copy: None,
    })
}

/// Downscales a freshly decoded full-resolution texture to the display size
/// the stage needs, so a 24 MP JPEG does not keep a 92 MB texture alive in the
/// viewer cache. Runs on the decode worker right before the result is sent.
fn fit_to_display(
    worker: &DecodeWorker,
    mut decoded: DecodedImage,
    key: &CacheKey,
) -> DecodedImage {
    let fit = [key.width.max(1), key.height.max(1)];
    match worker
        .downsampler
        .run(&decoded.view, [decoded.width, decoded.height], fit)
    {
        Ok((texture, view, [width, height])) => {
            log::info!(
                "downsample {}x{} fitted to {}x{} ({} px over {} px)",
                decoded.width,
                decoded.height,
                width,
                height,
                decoded.width * decoded.height,
                width * height
            );
            decoded.texture = texture;
            decoded.view = view;
            decoded.width = width;
            decoded.height = height;
            decoded
        }
        Err(_) => decoded,
    }
}

fn zero_copy_enabled() -> bool {
    std::env::var("GALLERY_ZERO_COPY")
        .map(|value| value != "0" && !value.eq_ignore_ascii_case("false"))
        .unwrap_or(true)
}
