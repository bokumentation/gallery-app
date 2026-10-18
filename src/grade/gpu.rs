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

/// Uniform-block mirror of `grade/shader.wgsl::ColorParams` (160 bytes).
const COLOR_SIZE: u64 = 160;
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

pub struct GradePass {
    device: Device,
    queue: Queue,
    render_state: RenderState,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    color: wgpu::Buffer,
    neutral_lut: wgpu::TextureView,
    sampler: wgpu::Sampler,
    luts: std::collections::HashMap<String, wgpu::TextureView>,
    last: Option<GradedTexture>,
}

impl GradePass {
    pub fn new(render_state: &RenderState) -> Self {
        let device = render_state.device.clone();
        let queue = render_state.queue.clone();
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
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
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
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        Self {
            device,
            queue,
            render_state: render_state.clone(),
            pipeline,
            layout,
            params,
            color,
            neutral_lut,
            sampler,
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
        let out_size: [u32; 2] = self.out_size(base_size, source_size, edit);
        let id = match self.dispatch(base, base_size, source_size, out_size, edit) {
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

    /// Output pixel size: crop dimensions in *base texture* space. Crop and
    /// rotation live in source-pixel space upstream, so they scale down with
    /// the displayed texture.
    fn out_size(
        &self,
        base_size: [u32; 2],
        source_size: [u32; 2],
        edit: &EditState,
    ) -> [u32; 2] {
        let scale = self.scale(base_size, source_size);
        edit.display_size(source_size)
            .map(|axis| ((axis as f32 * scale).round() as u32).max(1))
    }

    fn scale(&self, base_size: [u32; 2], source_size: [u32; 2]) -> f32 {
        if source_size[0] == 0 || source_size[1] == 0 {
            return 1.0;
        }
        (base_size[0] as f32 / source_size[0] as f32)
            .min(base_size[1] as f32 / source_size[1] as f32)
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch(
        &self,
        base: &wgpu::TextureView,
        base_size: [u32; 2],
        source_size: [u32; 2],
        out_size: [u32; 2],
        edit: &EditState,
    ) -> Result<TextureId, String> {
        let width = out_size[0].min(self.device.limits().max_texture_dimension_2d);
        let height = out_size[1].min(self.device.limits().max_texture_dimension_2d);
        let pitch = ((width as usize * 4 + 255) & !255).max(256) as u32;
        let crop = self.crop_in_base_space(base_size, source_size, edit);
        let bytes = self.param_bytes(width, height, pitch, crop, base_size, source_size, edit);

        let dispatch_lut = edit
            .lut
            .as_ref()
            .and_then(|name| self.luts.get(name));
        let lut_view = dispatch_lut.unwrap_or(&self.neutral_lut);
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
                    resource: wgpu::BindingResource::TextureView(lut_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
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
        let id = self
            .render_state
            .renderer
            .write()
            .register_native_texture(&self.device, &view, wgpu::FilterMode::Linear);
        self.queue.write_buffer(&self.params, 0, &bytes);
        self.queue
            .write_buffer(&self.color, 0, &self.color_bytes(edit));
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
        self.queue.submit(Some(encoder.finish()));
        Ok(id)
    }

    /// Crop coordinates in base-texture space (source pixels scaled down).
    fn crop_in_base_space(
        &self,
        base_size: [u32; 2],
        source_size: [u32; 2],
        edit: &EditState,
    ) -> CropRect {
        let scale = self.scale(base_size, source_size);
        let mut crop = edit.crop_rect(source_size);
        crop.x *= scale;
        crop.y *= scale;
        crop.w = (crop.w * scale).max(1.0);
        crop.h = (crop.h * scale).max(1.0);
        crop
    }

    #[allow(clippy::too_many_arguments)]
    fn param_bytes(
        &self,
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
        for (offset, value) in
            [crop.x, crop.y, crop.w, crop.h].into_iter().enumerate()
        {
            let offset = 16 + offset * 4;
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        // Oriented size in base-texture space, rotation degrees, steps.
        let oriented = edit.oriented_size(source_size);
        let scale = self.scale(base_size, source_size);
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
        for (offset, flag) in [(48, edit.flip_h as u32 as f32), (52, edit.flip_v as u32 as f32)] {
            bytes[offset..offset + 4].copy_from_slice(&flag.to_le_bytes());
        }
        bytes
    }

    /// Pre-scaled color values — `ColorParams` at 160 bytes: eight scalars,
    /// two 3x3 white-balance matrices, log gains and the two chroma sliders.
    /// The scaling table is the reference implementation's SCALES map.
    fn color_bytes(&self, edit: &EditState) -> [u8; COLOR_SIZE as usize] {
        let mut bytes = [0u8; COLOR_SIZE as usize];
        let adjustments = &edit.adjustments;

        let scalars: [(usize, f32); 8] = [
            (0, adjustments.exposure * 0.8),
            (4, adjustments.brightness * 0.8),
            (8, adjustments.contrast / 100.0),
            (12, adjustments.highlights / 120.0),
            (16, adjustments.shadows / 120.0),
            (20, adjustments.whites / 40.0),
            (24, adjustments.blacks / 40.0),
            (28, adjustments.hue),
        ];
        for (offset, value) in scalars {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }

        // White balance: reference D65 current, warmed/cooled by the slider.
        // mat3x3 uniform columns are 16-byte aligned: three floats then a
        // 4-byte pad per column.
        let rgb_to_lms = wb::rgb_to_lms_columns();
        let lms_to_rgb = wb::lms_to_rgb_columns();
        pack_matrix(&mut bytes, 32, rgb_to_lms);
        pack_matrix(&mut bytes, 80, lms_to_rgb);

        let current = wb::WhiteBalance::reference();
        let target = current.shifted(adjustments.temperature as f64, adjustments.tint as f64);
        let gains = wb::adaptation_log_gains(current, target);
        for (index, value) in gains.into_iter().enumerate() {
            let offset = 128 + index * 4;
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        for (offset, value) in [(144, adjustments.saturation / 100.0), (148, adjustments.vibrance / 100.0)] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        let has_lut: f32 = (edit.lut.is_some() && self.luts.contains_key(edit.lut.as_ref().unwrap()))
            as u32 as f32;
        bytes[152..156].copy_from_slice(&has_lut.to_le_bytes());
        bytes
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
        let texture = cube.upload(&self.device, &self.queue, "grade-lut");
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
