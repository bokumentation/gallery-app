//! GPU grade pass: one compute dispatch that applies geometry (orientation
//! steps, flips, free rotation, crop) and color grading to the base texture,
//! modelled on the zero-copy converter in `zerocopy.rs`.

use super::wb;
use super::{CropRect, EditState};
use egui::TextureId;
use egui_wgpu::RenderState;
use std::path::{Path, PathBuf};
use wgpu::Device;
use wgpu::Queue;

/// Uniform-block mirror of `grade/shader.wgsl::Params` (64 bytes).
const PARAMS_SIZE: u64 = 64;

/// Uniform-block mirror of `grade/shader.wgsl::ColorParams`. Byte offsets must
/// track the WGSL layout: `vec3<f32>` is 12 bytes (align 16), `mat3x3<f32>` is
/// three 16-byte columns, and uniform array elements have a 16-byte stride.
mod color_offset {
    pub const EXPOSURE: usize = 0;
    pub const BRIGHTNESS: usize = 4;
    pub const CONTRAST: usize = 8;
    pub const HIGHLIGHTS: usize = 12;
    pub const SHADOWS: usize = 16;
    pub const WHITES: usize = 20;
    pub const BLACKS: usize = 24;
    pub const HUE: usize = 28;
    pub const RGB_TO_LMS: usize = 32;
    pub const LMS_TO_RGB: usize = 80;
    pub const WB_GAINS: usize = 128;
    pub const SATURATION: usize = 140;
    pub const VIBRANCE: usize = 144;
    pub const HAS_LUT: usize = 148;
    pub const HSL: usize = 160;
    pub const COLOR_GRADING: usize = HSL + 8 * 16;
    pub const GRADING_BLENDING: usize = COLOR_GRADING + 4 * 16;
    pub const GRADING_BALANCE: usize = GRADING_BLENDING + 4;
}

/// Size of the uniform buffer: the last field end rounded up to the struct's
/// 16-byte alignment, matching WGSL.
const COLOR_SIZE: u64 = ((color_offset::GRADING_BALANCE + 4 + 15) & !15) as u64;
struct GradedTexture {
    id: TextureId,
    key: GradeKey,
}

#[derive(Clone, PartialEq)]
struct GradeKey {
    path: PathBuf,
    base_id: TextureId,
    base_size: [u32; 2],
    edit: EditState,
}

/// The GPU pipeline, uniforms and LUT sampler shared by the preview pass and
/// the offscreen export worker. Holds no egui state, so it can be sent to a
/// worker thread.
pub(crate) struct GradeCore {
    pub(crate) device: Device,
    pub(crate) queue: Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    color: wgpu::Buffer,
    neutral_lut: wgpu::TextureView,
    sampler: wgpu::Sampler,
}

impl GradeCore {
    pub(crate) fn new(device: &Device, queue: &Queue) -> Self {
        let device = device.clone();
        let queue = queue.clone();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grade-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grade-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grade-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("grade-pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grade-params"),
            size: PARAMS_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let color = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grade-color"),
            size: COLOR_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // 1x1x1 placeholder bound whenever no LUT is attached (the shader
        // skips sampling via `has_lut`).
        let neutral = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("grade-neutral-lut"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let neutral_lut = neutral.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("grade-lut-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        Self {
            device,
            queue,
            pipeline,
            layout,
            params,
            color,
            neutral_lut,
            sampler,
        }
    }

    fn scale(base_size: [u32; 2], source_size: [u32; 2]) -> f32 {
        if source_size[0] == 0 || source_size[1] == 0 {
            return 1.0;
        }
        (base_size[0] as f32 / source_size[0] as f32)
            .min(base_size[1] as f32 / source_size[1] as f32)
    }

    /// Output pixel size: crop dimensions in *base texture* space. Crop and
    /// rotation live in source-pixel space upstream, so they scale down with
    /// the displayed texture.
    fn out_size(base_size: [u32; 2], source_size: [u32; 2], edit: &EditState) -> [u32; 2] {
        let scale = Self::scale(base_size, source_size);
        edit.display_size(source_size)
            .map(|axis| ((axis as f32 * scale).round() as u32).max(1))
    }

    /// Crop coordinates in base-texture space (source pixels scaled down).
    fn crop_in_base_space(
        base_size: [u32; 2],
        source_size: [u32; 2],
        edit: &EditState,
    ) -> CropRect {
        let scale = Self::scale(base_size, source_size);
        let mut crop = edit.crop_rect(source_size);
        crop.x *= scale;
        crop.y *= scale;
        crop.w = (crop.w * scale).max(1.0);
        crop.h = (crop.h * scale).max(1.0);
        crop
    }

    /// Record the grade compute pass into a fresh encoder, writing RGBA into a
    /// storage buffer. Returns the buffer, its dimensions, row pitch and the
    /// encoder (which the caller finishes after adding copy commands).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn encode(
        &self,
        base: &wgpu::TextureView,
        base_size: [u32; 2],
        source_size: [u32; 2],
        edit: &EditState,
        has_lut_resident: bool,
        lut_view: Option<&wgpu::TextureView>,
    ) -> Result<(wgpu::Buffer, u32, u32, u32, wgpu::CommandEncoder), String> {
        let out = Self::out_size(base_size, source_size, edit);
        let width = out[0].min(self.device.limits().max_texture_dimension_2d);
        let height = out[1].min(self.device.limits().max_texture_dimension_2d);
        let pitch = ((width as usize * 4 + 255) & !255).max(256) as u32;
        let crop = Self::crop_in_base_space(base_size, source_size, edit);
        let bytes = param_bytes(width, height, pitch, crop, base_size, source_size, edit);

        let lut = lut_view.unwrap_or(&self.neutral_lut);
        let destination = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grade-output"),
            size: pitch as u64 * height as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grade-bind-group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(base),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.color.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: destination.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(lut),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.queue.write_buffer(&self.params, 0, &bytes);
        self.queue
            .write_buffer(&self.color, 0, &pack_color(edit, has_lut_resident));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("grade-encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("grade-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        Ok((destination, width, height, pitch, encoder))
    }
}

pub struct GradePass {
    core: GradeCore,
    render_state: RenderState,
    luts: std::collections::HashMap<String, wgpu::TextureView>,
    last: Option<GradedTexture>,
}

impl GradePass {
    pub fn new(render_state: &RenderState) -> Self {
        Self {
            core: GradeCore::new(&render_state.device, &render_state.queue),
            render_state: render_state.clone(),
            luts: std::collections::HashMap::new(),
            last: None,
        }
    }

    /// Cached result for `(path, base_id, base_size, edit)`, computing a fresh
    /// graded texture only when anything changed.
    pub fn ensure(
        &mut self,
        path: &Path,
        base: &wgpu::TextureView,
        base_id: TextureId,
        base_size: [u32; 2],
        source_size: [u32; 2],
        edit: &EditState,
    ) -> Option<TextureId> {
        let key = GradeKey {
            path: path.to_path_buf(),
            base_id,
            base_size,
            edit: edit.clone(),
        };
        if let Some(current) = &self.last {
            if current.key == key {
                return Some(current.id);
            }
        }
        let started = std::time::Instant::now();
        let out_size: [u32; 2] = GradeCore::out_size(base_size, source_size, edit);
        let id = match self.dispatch(base, base_size, source_size, edit) {
            Ok(id) => id,
            Err(error) => {
                log::warn!("grade pass failed: {error}");
                return None;
            }
        };
        if let Some(previous) = self.last.take() {
            self.release_texture(previous.id);
        }
        self.last = Some(GradedTexture {
            id,
            key,
        });
        log::debug!(
            "grade pass {}x{} in {:.2} ms",
            out_size[0],
            out_size[1],
            started.elapsed().as_secs_f64() * 1000.0
        );
        Some(id)
    }

    /// Frees the cached graded texture (used when the viewer closes).
    pub fn release(&mut self) {
        if let Some(current) = self.last.take() {
            self.release_texture(current.id);
        }
    }

    fn release_texture(&self, id: TextureId) {
        self.render_state.renderer.write().free_texture(&id);
    }

    /// Clone of the wgpu device, for the export worker (which builds its own
    /// offscreen pipeline rather than touching egui state).
    pub fn device(&self) -> Device {
        self.core.device.clone()
    }

    /// Clone of the wgpu queue, paired with [`GradePass::device`].
    pub fn queue(&self) -> Queue {
        self.core.queue.clone()
    }

    /// Register a graded texture for the preview, reusing the shared core.
    fn dispatch(
        &self,
        base: &wgpu::TextureView,
        base_size: [u32; 2],
        source_size: [u32; 2],
        edit: &EditState,
    ) -> Result<TextureId, String> {
        let has_lut_resident = edit
            .lut
            .as_ref()
            .is_some_and(|name| self.luts.contains_key(name));
        let lut_view = edit.lut.as_ref().and_then(|name| self.luts.get(name));
        let (destination, width, height, pitch, mut encoder) = self.core.encode(
            base,
            base_size,
            source_size,
            edit,
            has_lut_resident,
            lut_view,
        )?;
        let texture = self
            .core
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("grade-output-texture"),
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
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &destination,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch),
                    rows_per_image: Some(height),
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.core.queue.submit(Some(encoder.finish()));
        Ok(self
            .render_state
            .renderer
            .write()
            .register_native_texture(&self.core.device, &view, wgpu::FilterMode::Linear))
    }
}

#[allow(clippy::too_many_arguments)]
fn param_bytes(
    width: u32,
    height: u32,
    pitch: u32,
    crop: CropRect,
    base_size: [u32; 2],
    source_size: [u32; 2],
    edit: &EditState,
) -> [u8; PARAMS_SIZE as usize] {
    let mut bytes = [0u8; PARAMS_SIZE as usize];
    // out_size + pitch + pad.
    bytes[0..4].copy_from_slice(&width.to_le_bytes());
    bytes[4..8].copy_from_slice(&height.to_le_bytes());
    bytes[8..12].copy_from_slice(&pitch.to_le_bytes());
    // Crop rectangle in base-texture space.
    for (offset, value) in [crop.x, crop.y, crop.w, crop.h].into_iter().enumerate() {
        let offset = 16 + offset * 4;
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    // Oriented size in base-texture space, rotation degrees, steps.
    let oriented = edit.oriented_size(source_size);
    let scale = GradeCore::scale(base_size, source_size);
    let oriented = oriented.map(|axis| axis as f32 * scale);
    for (offset, value) in [
        oriented[0],
        oriented[1],
        edit.rotation,
        edit.orientation_steps as f32,
    ]
    .into_iter()
    .enumerate()
    {
        let offset = 32 + offset * 4;
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    // Flip flags.
    for (offset, flag) in [
        (48, edit.flip_h as u32 as f32),
        (52, edit.flip_v as u32 as f32),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&flag.to_le_bytes());
    }
    bytes
}

/// Pack [`EditState`] into the shader `ColorParams` uniform. Byte offsets come
/// from [`color_offset`], which mirrors the WGSL struct layout; the scaling
/// table is the reference implementation's SCALES map.
fn pack_color(edit: &EditState, has_lut_resident: bool) -> [u8; COLOR_SIZE as usize] {
    let mut bytes = [0u8; COLOR_SIZE as usize];
    let adjustments = &edit.adjustments;
    let put = |bytes: &mut [u8], offset: usize, value: f32| {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    };

    let scalars: [(usize, f32); 8] = [
        (color_offset::EXPOSURE, adjustments.exposure * 0.8),
        (color_offset::BRIGHTNESS, adjustments.brightness * 0.8),
        (color_offset::CONTRAST, adjustments.contrast / 100.0),
        (color_offset::HIGHLIGHTS, adjustments.highlights / 120.0),
        (color_offset::SHADOWS, adjustments.shadows / 120.0),
        (color_offset::WHITES, adjustments.whites / 40.0),
        (color_offset::BLACKS, adjustments.blacks / 40.0),
        (color_offset::HUE, adjustments.hue),
    ];
    for (offset, value) in scalars {
        put(&mut bytes, offset, value);
    }

    // White balance: reference D65 current, warmed/cooled by the slider.
    // mat3x3 uniform columns are 16-byte aligned: three floats then a
    // 4-byte pad per column.
    let rgb_to_lms = wb::rgb_to_lms_columns();
    let lms_to_rgb = wb::lms_to_rgb_columns();
    pack_matrix(&mut bytes, color_offset::RGB_TO_LMS, rgb_to_lms);
    pack_matrix(&mut bytes, color_offset::LMS_TO_RGB, lms_to_rgb);

    let current = wb::WhiteBalance::reference();
    let target = current.shifted(adjustments.temperature as f64, adjustments.tint as f64);
    let gains = wb::adaptation_log_gains(current, target);
    for (index, value) in gains.into_iter().enumerate() {
        put(&mut bytes, color_offset::WB_GAINS + index * 4, value);
    }
    put(&mut bytes, color_offset::SATURATION, adjustments.saturation / 100.0);
    put(&mut bytes, color_offset::VIBRANCE, adjustments.vibrance / 100.0);
    let has_lut = has_lut_resident as u32 as f32;
    put(&mut bytes, color_offset::HAS_LUT, has_lut);

    // HSL mixer: hue scaled by the reference's 0.3 multiplier, saturation and
    // luminance by 1/100.
    for (index, hsl) in adjustments.hsl.iter().enumerate() {
        let base = color_offset::HSL + index * 16;
        put(&mut bytes, base, hsl.hue * 0.3);
        put(&mut bytes, base + 4, hsl.saturation / 100.0);
        put(&mut bytes, base + 8, hsl.luminance / 100.0);
    }

    // Color grading wheels: saturation/luminance by 1/500; master blending by
    // 1/100 and balance by 1/200.
    let grading = &adjustments.color_grading;
    for (index, wheel) in grading.wheels.iter().enumerate() {
        let base = color_offset::COLOR_GRADING + index * 16;
        put(&mut bytes, base, wheel.hue);
        put(&mut bytes, base + 4, wheel.saturation / 500.0);
        put(&mut bytes, base + 8, wheel.luminance / 500.0);
    }
    put(
        &mut bytes,
        color_offset::GRADING_BLENDING,
        grading.blending / 100.0,
    );
    put(
        &mut bytes,
        color_offset::GRADING_BALANCE,
        grading.balance / 200.0,
    );
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grade::HslRange;

    fn read(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn color_params_matches_wgsl_layout() {
        // Mirrors ColorParams in shader.wgsl; if either side moves these
        // offsets the packed uniform silently desynchronises, so keep in sync.
        assert_eq!(color_offset::EXPOSURE, 0);
        assert_eq!(color_offset::RGB_TO_LMS, 32);
        assert_eq!(color_offset::LMS_TO_RGB, 80);
        assert_eq!(color_offset::WB_GAINS, 128);
        assert_eq!(color_offset::SATURATION, 140);
        assert_eq!(color_offset::VIBRANCE, 144);
        assert_eq!(color_offset::HAS_LUT, 148);
        assert_eq!(color_offset::HSL, 160);
        assert_eq!(color_offset::COLOR_GRADING, 288);
        assert_eq!(color_offset::GRADING_BLENDING, 352);
        assert_eq!(color_offset::GRADING_BALANCE, 356);
        assert_eq!(COLOR_SIZE, 368);
    }

    #[test]
    fn color_params_packs_each_control_at_its_offset() {
        let mut edit = EditState::default();
        edit.adjustments.saturation = 75.0;
        edit.adjustments.vibrance = -50.0;
        edit.adjustments.hue = 42.0;
        edit.adjustments.hsl[3] = HslRange {
            hue: 50.0,
            saturation: 30.0,
            luminance: -20.0,
        };
        edit.adjustments.color_grading.wheels[2] = HslRange {
            hue: 210.0,
            saturation: 200.0,
            luminance: -100.0,
        };
        edit.adjustments.color_grading.blending = 60.0;
        edit.adjustments.color_grading.balance = 40.0;

        let bytes = pack_color(&edit, true);

        assert!((read(&bytes, color_offset::SATURATION) - 0.75).abs() < 1e-6);
        assert!((read(&bytes, color_offset::VIBRANCE) + 0.5).abs() < 1e-6);
        assert!((read(&bytes, color_offset::HAS_LUT) - 1.0).abs() < 1e-6);
        assert!((read(&bytes, color_offset::HUE) - 42.0).abs() < 1e-6);

        let hsl = color_offset::HSL + 3 * 16;
        assert!((read(&bytes, hsl) - 15.0).abs() < 1e-6); // 50 * 0.3
        assert!((read(&bytes, hsl + 4) - 0.30).abs() < 1e-6);
        assert!((read(&bytes, hsl + 8) + 0.20).abs() < 1e-6);

        let wheel = color_offset::COLOR_GRADING + 2 * 16;
        assert!((read(&bytes, wheel) - 210.0).abs() < 1e-6);
        assert!((read(&bytes, wheel + 4) - 0.4).abs() < 1e-6);
        assert!((read(&bytes, wheel + 8) + 0.2).abs() < 1e-6);

        assert!((read(&bytes, color_offset::GRADING_BLENDING) - 0.6).abs() < 1e-6);
        assert!((read(&bytes, color_offset::GRADING_BALANCE) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn color_params_lut_flag_requires_resident_lut() {
        let mut edit = EditState::default();
        edit.lut = Some("Kodak 2383".to_owned());
        assert_eq!(read(&pack_color(&edit, false), color_offset::HAS_LUT), 0.0);
        assert_eq!(read(&pack_color(&edit, true), color_offset::HAS_LUT), 1.0);
    }
}

/// Flatten column-major `[f32; 9]` into a WGSL `mat3x3` uniform layout: each
/// of the three columns takes 16 bytes (three floats plus a pad).
fn pack_matrix(bytes: &mut [u8], at: usize, columns: [f32; 9]) {
    for column in 0..3 {
        for row in 0..3 {
            let offset = at + column * 16 + row * 4;
            let value = columns[column * 3 + row];
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
}

impl GradePass {
    /// Parse, persist and upload a `.cube` LUT; returns the name on success.
    pub fn import_lut(&mut self, cube: &crate::luts::CubeLut) -> Option<String> {
        let texture = cube.upload(&self.core.device, &self.core.queue, "grade-lut");
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.luts.insert(cube.name.clone(), view);
        Some(cube.name.clone())
    }

    /// True when a persisted LUT name is already resident on the GPU.
    #[allow(dead_code)]
    pub fn has_lut(&self, name: &str) -> bool {
        self.luts.contains_key(name)
    }
}
