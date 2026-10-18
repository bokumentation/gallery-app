//! Full-resolution export.
//!
//! The preview grades a downsampled base texture; export re-decodes the source
//! at full size, runs the same compute pipeline offscreen (see
//! [`crate::grade::gpu::GradeCore`]), reads the result back and writes a JPEG
//! plus a JSON sidecar describing the edit. The heavy work (decode, GPU
//! dispatch, readback, encode) runs on a worker thread so egui never blocks.

use crate::grade::gpu::GradeCore;
use crate::grade::{CropRect, EditState};
use crate::thumbs::decode_image;
use crossbeam_channel::{unbounded, Receiver};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use wgpu::{Device, Queue};

/// JPEG quality for exported files.
const JPEG_QUALITY: u8 = 92;

pub struct ExportOutcome {
    /// Written image and sidecar paths, or the error that stopped the export.
    pub result: Result<ExportPaths, String>,
    pub elapsed: Duration,
}

pub struct ExportPaths {
    pub image: PathBuf,
    pub sidecar: PathBuf,
}

/// Run one export on a worker thread. The returned receiver yields exactly one
/// outcome.
pub fn spawn(
    device: Device,
    queue: Queue,
    path: PathBuf,
    edit: EditState,
) -> Receiver<ExportOutcome> {
    let (tx, rx) = unbounded();
    std::thread::Builder::new()
        .name("export".to_owned())
        .spawn(move || {
            let started = Instant::now();
            let result = run(&device, &queue, &path, &edit);
            let _ = tx.send(ExportOutcome {
                result,
                elapsed: started.elapsed(),
            });
        })
        .expect("spawn export worker");
    rx
}

fn run(
    device: &Device,
    queue: &Queue,
    path: &Path,
    edit: &EditState,
) -> Result<ExportPaths, String> {
    let decoded = decode_image(path)?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return Err("image has no pixels".to_owned());
    }
    let source_size = [width, height];

    let (pixels, out_width, out_height) = if edit.is_identity() {
        (rgba.into_raw(), width, height)
    } else {
        grade_pixels(device, queue, rgba.into_raw(), source_size, edit)?
    };

    let (image_path, sidecar_path) = output_paths(path);
    // JPEG has no alpha channel; drop it.
    let rgb: Vec<u8> = pixels
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    // Encode into memory first so a failed encode cannot leave a truncated or
    // 0-byte file behind (cropped/rotated output sizes differ from the source,
    // which the encoder must be told explicitly).
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY)
        .encode(&rgb, out_width, out_height, image::ExtendedColorType::Rgb8)
        .map_err(|err| format!("encode JPEG ({out_width}x{out_height}): {err}"))?;
    std::fs::write(&image_path, &jpeg)
        .map_err(|err| format!("write {}: {err}", image_path.display()))?;
    std::fs::write(&sidecar_path, edit_sidecar(path, edit))
        .map_err(|err| format!("write {}: {err}", sidecar_path.display()))?;

    Ok(ExportPaths {
        image: image_path,
        sidecar: sidecar_path,
    })
}

/// Upload the full-resolution pixels, dispatch the shared grade compute pass,
/// and copy the result back to the host. Returns the pixels plus the output
/// dimensions (which differ from the source after a crop or 90/270 rotation).
fn grade_pixels(
    device: &Device,
    queue: &Queue,
    rgba: Vec<u8>,
    source_size: [u32; 2],
    edit: &EditState,
) -> Result<(Vec<u8>, u32, u32), String> {
    let core = GradeCore::new(device, queue);

    let base = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("export-base"),
        size: wgpu::Extent3d {
            width: source_size[0],
            height: source_size[1],
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
            texture: &base,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(source_size[0] * 4),
            rows_per_image: Some(source_size[1]),
        },
        wgpu::Extent3d {
            width: source_size[0],
            height: source_size[1],
            depth_or_array_layers: 1,
        },
    );
    drop(rgba);
    let base_view = base.create_view(&wgpu::TextureViewDescriptor::default());

    // Attach the LUT if the recipe names one and it loads from the store.
    let lut_texture = match edit.lut.as_deref() {
        Some(name) => {
            crate::luts::CubeLut::load(name).map(|cube| cube.upload(device, queue, "export-lut"))
        }
        None => None,
    };
    let lut_view = lut_texture
        .as_ref()
        .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()));
    let has_lut_resident = lut_view.is_some();

    let (destination, width, height, pitch, mut encoder) = core.encode(
        &base_view,
        source_size,
        source_size,
        edit,
        has_lut_resident,
        lut_view.as_ref(),
    )?;

    let byte_len = pitch as u64 * height as u64;
    let limit = device.limits().max_storage_buffer_binding_size as u64;
    if byte_len > limit {
        return Err(format!(
            "image too large for GPU export ({width}x{height}); lower the resolution"
        ));
    }

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("export-readback"),
        size: byte_len,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_buffer_to_buffer(&destination, 0, &readback, 0, byte_len);
    queue.submit(Some(encoder.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .map_err(|err| format!("poll device: {err}"))?;
    rx.recv()
        .map_err(|err| format!("map callback: {err}"))?
        .map_err(|err| format!("map buffer: {err}"))?;

    let mapped = slice
        .get_mapped_range()
        .map_err(|err| format!("read buffer: {err}"))?;
    // Strip the 256-byte row pitch padding to a tight RGBA image.
    let row = width as usize * 4;
    let mut pixels = Vec::with_capacity(row * height as usize);
    for y in 0..height as usize {
        let start = y * pitch as usize;
        pixels.extend_from_slice(&mapped[start..start + row]);
    }
    drop(mapped);
    readback.unmap();
    Ok((pixels, width, height))
}

/// `<dir>/<stem>_edited.jpg` and `.json` next to the source.
fn output_paths(path: &Path) -> (PathBuf, PathBuf) {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".to_owned());
    (
        dir.join(format!("{stem}_edited.jpg")),
        dir.join(format!("{stem}_edited.json")),
    )
}

/// Hand-written JSON sidecar (the crate keeps its dependency list minimal; the
/// schema is flat enough that a manual writer beats pulling in serde).
fn edit_sidecar(path: &Path, edit: &EditState) -> String {
    let a = &edit.adjustments;
    let cg = &a.color_grading;
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str("  \"app\": \"gallery-app\",\n");
    json.push_str("  \"version\": 1,\n");
    json.push_str(&format!(
        "  \"source\": {},\n",
        json_string(&path.display().to_string())
    ));
    json.push_str("  \"adjustments\": {\n");
    json.push_str(&format!("    \"exposure\": {},\n", a.exposure));
    json.push_str(&format!("    \"brightness\": {},\n", a.brightness));
    json.push_str(&format!("    \"contrast\": {},\n", a.contrast));
    json.push_str(&format!("    \"highlights\": {},\n", a.highlights));
    json.push_str(&format!("    \"shadows\": {},\n", a.shadows));
    json.push_str(&format!("    \"whites\": {},\n", a.whites));
    json.push_str(&format!("    \"blacks\": {},\n", a.blacks));
    json.push_str(&format!("    \"temperature\": {},\n", a.temperature));
    json.push_str(&format!("    \"tint\": {},\n", a.tint));
    json.push_str(&format!("    \"hue\": {},\n", a.hue));
    json.push_str(&format!("    \"saturation\": {},\n", a.saturation));
    json.push_str(&format!("    \"vibrance\": {}\n", a.vibrance));
    json.push_str("  },\n");
    json.push_str("  \"hsl\": {\n");
    for (index, name) in crate::grade::HSL_NAMES.iter().enumerate() {
        let range = a.hsl[index];
        json.push_str(&format!(
            "    \"{}\": {{ \"hue\": {}, \"saturation\": {}, \"luminance\": {} }}{}\n",
            name.to_lowercase(),
            range.hue,
            range.saturation,
            range.luminance,
            if index + 1 == crate::grade::HSL_NAMES.len() {
                ""
            } else {
                ","
            }
        ));
    }
    json.push_str("  },\n");
    json.push_str("  \"colorGrading\": {\n");
    const WHEELS: [&str; 4] = ["shadows", "midtones", "highlights", "global"];
    for (index, name) in WHEELS.iter().enumerate() {
        let wheel = cg.wheels[index];
        json.push_str(&format!(
            "    \"{}\": {{ \"hue\": {}, \"saturation\": {}, \"luminance\": {} }},\n",
            name, wheel.hue, wheel.saturation, wheel.luminance
        ));
    }
    json.push_str(&format!("    \"blending\": {},\n", cg.blending));
    json.push_str(&format!("    \"balance\": {}\n", cg.balance));
    json.push_str("  },\n");
    json.push_str("  \"geometry\": {\n");
    json.push_str(&format!("    \"rotation\": {},\n", edit.rotation));
    json.push_str(&format!(
        "    \"orientationSteps\": {},\n",
        edit.orientation_steps
    ));
    json.push_str(&format!("    \"flipH\": {},\n", edit.flip_h));
    json.push_str(&format!("    \"flipV\": {},\n", edit.flip_v));
    json.push_str(&format!(
        "    \"crop\": {},\n",
        edit.crop
            .map(crop_json)
            .unwrap_or_else(|| "null".to_owned())
    ));
    json.push_str(&format!(
        "    \"aspect\": {}\n",
        edit.aspect
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_owned())
    ));
    json.push_str("  },\n");
    json.push_str(&format!(
        "  \"lut\": {}\n",
        edit.lut
            .as_deref()
            .map(json_string)
            .unwrap_or_else(|| "null".to_owned())
    ));
    json.push_str("}\n");
    json
}

fn crop_json(crop: CropRect) -> String {
    format!(
        "{{ \"x\": {}, \"y\": {}, \"w\": {}, \"h\": {} }}",
        crop.x, crop.y, crop.w, crop.h
    )
}

/// Minimal JSON string escaping for the paths and LUT names we emit.
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grade::HslRange;

    /// Minimal executor so the wgpu async adapter/device requests can be driven
    /// from a synchronous test without pulling in pollster.
    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
        fn noop(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
        let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);
        let mut fut = Box::pin(fut);
        loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    /// Create a device + queue, or `None` when the machine has no adapter.
    fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }

    /// Exercise the real offscreen grade + readback path. Skips (rather than
    /// fails) when the machine has no usable wgpu adapter.
    #[test]
    fn grade_readback_pipeline() {
        let Some((device, queue)) = gpu() else {
            eprintln!("no wgpu adapter; skipping grade readback test");
            return;
        };

        // 4x4 opaque input: opaque red gradient with distinct channels.
        let mut rgba = Vec::with_capacity(4 * 4 * 4);
        for y in 0..4u8 {
            for x in 0..4u8 {
                rgba.extend_from_slice(&[200, 60 + x * 40, 30 + y * 30, 255]);
            }
        }

        // Crop to the inner 2x2: output is exactly those pixels, graded.
        let mut edit = EditState::default();
        edit.crop = Some(CropRect {
            x: 1.0,
            y: 1.0,
            w: 2.0,
            h: 2.0,
        });
        edit.adjustments.saturation = 100.0;
        let (graded, out_w, out_h) =
            grade_pixels(&device, &queue, rgba.clone(), [4, 4], &edit).expect("grade + readback");
        assert_eq!((out_w, out_h), (2, 2), "crop output size");
        assert_eq!(graded.len(), 2 * 2 * 4, "crop output length");

        // Saturation should have changed the cropped pixels; alpha stays opaque.
        let source_pixel = &rgba[(1 * 4 + 1) * 4..(1 * 4 + 1) * 4 + 4];
        assert_ne!(
            &graded[..3],
            &source_pixel[..3],
            "saturation changed pixels"
        );
        assert_eq!(graded[3], 255);
    }

    /// Pin the 90-degree orientation mapping. Input is a 4x2 field where
    /// `R = x * 50` and `G = y * 100`; with one clockwise step the shader must
    /// read `src = (y, oriented.x - x)` in oriented space, so the output column
    /// `x' = 0` corresponds to source column `y' = 1` (G ≈ 100) and `x' = 1` to
    /// source column `0` (G ≈ 0). The old `oriented.y - x` mapping read the
    /// wrong column (G ≈ 255).
    #[test]
    fn orientation_step_maps_source_correctly() {
        let Some((device, queue)) = gpu() else {
            eprintln!("no wgpu adapter; skipping orientation test");
            return;
        };
        let mut rgba = Vec::with_capacity(4 * 2 * 4);
        for y in 0..2u8 {
            for x in 0..4u8 {
                rgba.extend_from_slice(&[x * 50, y * 100, 128, 255]);
            }
        }
        let mut edit = EditState::default();
        edit.orientation_steps = 1;
        let (out, out_w, out_h) =
            grade_pixels(&device, &queue, rgba, [4, 2], &edit).expect("grade + readback");
        // Oriented size is 2x4 (width x height).
        assert_eq!((out_w, out_h), (2, 4));
        assert_eq!(out.len(), 2 * 4 * 4);
        let at = |x: usize, y: usize| -> [u8; 4] {
            let i = (y * 2 + x) * 4;
            out[i..i + 4].try_into().unwrap()
        };
        // Output (0,0) should come from source (0,1): G ≈ 100. Output (1,0)
        // from source (0,0): G ≈ 0.
        assert!((at(0, 0)[1] as i32 - 100).abs() <= 6, "got {:?}", at(0, 0));
        assert!(at(1, 0)[1] <= 6, "got {:?}", at(1, 0));
        assert!(at(0, 0)[0] <= 6, "got {:?}", at(0, 0));
    }

    /// Unique temp dir for an export test, with a synthetic PNG of `w` x `h`.
    fn temp_png(width: u32, height: u32) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "gallery-export-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("sample.png");
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&[
                    (40 + x * 20 % 200) as u8,
                    (30 + y * 20 % 200) as u8,
                    160,
                    255,
                ]);
            }
        }
        image::RgbaImage::from_raw(width, height, rgba)
            .unwrap()
            .save(&source)
            .unwrap();
        (dir, source)
    }

    /// Assert the exported JPEG decodes to `width` x `height` and is non-empty,
    /// and that the sidecar was written. This is the regression guard for the
    /// 0-byte crop bug: the encoder must be told the *output* dimensions.
    fn assert_export(paths: &ExportPaths, width: u32, height: u32) {
        let bytes = std::fs::read(&paths.image).unwrap();
        assert!(!bytes.is_empty(), "exported JPEG must not be empty");
        let decoded = image::open(&paths.image).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (width, height));
        assert!(paths.sidecar.exists(), "sidecar written");
    }

    /// Full export path on a synthetic PNG: decode, grade, JPEG + sidecar.
    #[test]
    fn run_writes_jpeg_and_sidecar() {
        let Some((device, queue)) = gpu() else {
            eprintln!("no wgpu adapter; skipping export end-to-end test");
            return;
        };
        let (dir, source) = temp_png(8, 8);
        let mut edit = EditState::default();
        edit.adjustments.saturation = 50.0;
        let paths = run(&device, &queue, &source, &edit).expect("export");
        assert_export(&paths, 8, 8);
        let sidecar = std::fs::read_to_string(&paths.sidecar).unwrap();
        assert!(sidecar.contains("\"saturation\": 50"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Cropping shrinks the output: the JPEG must be encoded at the crop size,
    /// not the source size (the old code passed the source dims and failed with
    /// a buffer-length error, leaving a 0-byte file).
    #[test]
    fn run_exports_cropped_dimensions() {
        let Some((device, queue)) = gpu() else {
            eprintln!("no wgpu adapter; skipping crop export test");
            return;
        };
        let (dir, source) = temp_png(8, 8);
        let mut edit = EditState::default();
        edit.crop = Some(CropRect {
            x: 2.0,
            y: 1.0,
            w: 4.0,
            h: 3.0,
        });
        let paths = run(&device, &queue, &source, &edit).expect("cropped export");
        assert_export(&paths, 4, 3);
        let sidecar = std::fs::read_to_string(&paths.sidecar).unwrap();
        assert!(sidecar.contains("\"crop\": { \"x\": 2"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A 90-degree step swaps the output dimensions; the JPEG must follow.
    #[test]
    fn run_exports_rotated_dimensions() {
        let Some((device, queue)) = gpu() else {
            eprintln!("no wgpu adapter; skipping rotation export test");
            return;
        };
        let (dir, source) = temp_png(8, 6);
        let mut edit = EditState::default();
        edit.orientation_steps = 1;
        let paths = run(&device, &queue, &source, &edit).expect("rotated export");
        assert_export(&paths, 6, 8);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// HSL mixer uniform: desaturating the red range must pull a red pixel
    /// towards grey, proving `hsl[]` lands at the offsets the shader reads.
    #[test]
    fn hsl_mixer_desaturates_matching_hue() {
        let Some((device, queue)) = gpu() else {
            eprintln!("no wgpu adapter; skipping HSL test");
            return;
        };
        let rgba = [220u8, 30, 30, 255].repeat(4);
        let mut edit = EditState::default();
        edit.adjustments.hsl[0].saturation = -100.0;
        let (out, _, _) = grade_pixels(&device, &queue, rgba, [2, 2], &edit).expect("grade");
        // Grey has R ≈ G ≈ B; the source red is far from that.
        let spread = |p: &[u8]| p.iter().take(3).max().unwrap() - p.iter().take(3).min().unwrap();
        assert!(
            spread(&out[0..4]) + 20 < spread(&[220u8, 30, 30]),
            "red should desaturate, got {:?}",
            &out[0..4]
        );
    }

    /// Color-grading uniform: a blue highlight wheel must push a bright grey
    /// towards blue, proving `color_grading[]` lands at the right offsets.
    #[test]
    fn color_grading_tints_highlights() {
        let Some((device, queue)) = gpu() else {
            eprintln!("no wgpu adapter; skipping color grading test");
            return;
        };
        let rgba = [230u8, 230, 230, 255].repeat(4);
        let mut edit = EditState::default();
        edit.adjustments.color_grading.wheels[2] = HslRange {
            hue: 240.0,
            saturation: 100.0,
            luminance: 0.0,
        };
        let (out, _, _) = grade_pixels(&device, &queue, rgba, [2, 2], &edit).expect("grade");
        let pixel = &out[0..4];
        assert!(
            pixel[2] as i32 > pixel[0] as i32 + 5,
            "highlights should turn blue, got {:?}",
            pixel
        );
    }

    #[test]
    fn output_paths_sit_next_to_source() {
        let (image, sidecar) = output_paths(Path::new("/photos/IMG_0001.jpg"));
        assert_eq!(image, PathBuf::from("/photos/IMG_0001_edited.jpg"));
        assert_eq!(sidecar, PathBuf::from("/photos/IMG_0001_edited.json"));
    }

    #[test]
    fn sidecar_is_valid_flat_json() {
        let edit = EditState::default();
        let text = edit_sidecar(Path::new("/photos/a.jpg"), &edit);
        assert!(text.starts_with('{'));
        assert!(text.trim_end().ends_with('}'));
        assert!(text.contains("\"source\": \"/photos/a.jpg\""));
        assert!(text.contains("\"lut\": null"));
        assert!(text.contains("\"crop\": null"));
        // Global hue + eight HSL ranges + four grading wheels.
        assert_eq!(text.matches("\"hue\": 0").count(), 13);
    }

    #[test]
    fn json_string_escapes() {
        assert_eq!(json_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }
}
