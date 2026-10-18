use crate::nvjpeg::NvJpegDecoder;
use cudarc::driver::{DevicePtr, MappedBuffer};
use libloading::Library;
use std::ffi::c_void;
use std::fs::File;
use std::os::fd::FromRawFd;
use std::path::Path;
use std::time::{Duration, Instant};
use wgpu::{Device, Queue};

type GetMemoryProperties =
    unsafe extern "system" fn(ash::vk::PhysicalDevice, *mut ash::vk::PhysicalDeviceMemoryProperties);
type GetDeviceProcAddr =
    unsafe extern "system" fn(ash::vk::Device, *const std::os::raw::c_char) -> *const c_void;
type GetMemoryFd = unsafe extern "system" fn(
    ash::vk::Device,
    *const ash::vk::MemoryGetFdInfoKHR<'_>,
    *mut std::os::raw::c_int,
) -> ash::vk::Result;
type FreeMemory = unsafe extern "system" fn(
    ash::vk::Device,
    ash::vk::DeviceMemory,
    *const ash::vk::AllocationCallbacks<'_>,
) -> ash::vk::Result;

struct VulkanFns {
    get_memory_properties: GetMemoryProperties,
    get_device_proc_addr: GetDeviceProcAddr,
    _library: Library,
}

impl VulkanFns {
    fn load() -> Result<Self, String> {
        let library = unsafe { Library::new("libvulkan.so.1") }
            .map_err(|error| format!("load libvulkan: {error}"))?;
        unsafe {
            let get_memory_properties = *library
                .get::<GetMemoryProperties>(b"vkGetPhysicalDeviceMemoryProperties\0")
                .map_err(|error| format!("vkGetPhysicalDeviceMemoryProperties: {error}"))?;
            let get_device_proc_addr = *library
                .get::<GetDeviceProcAddr>(b"vkGetDeviceProcAddr\0")
                .map_err(|error| format!("vkGetDeviceProcAddr: {error}"))?;
            Ok(Self {
                get_memory_properties,
                get_device_proc_addr,
                _library: library,
            })
        }
    }

    fn memory_fd(&self, device: ash::vk::Device) -> Result<GetMemoryFd, String> {
        let pointer = unsafe {
            (self.get_device_proc_addr)(device, b"vkGetMemoryFdKHR\0".as_ptr().cast())
        };
        if pointer.is_null() {
            return Err("vkGetMemoryFdKHR is unavailable".to_owned());
        }
        Ok(unsafe { std::mem::transmute(pointer) })
    }

    fn free_memory(&self, device: ash::vk::Device) -> Result<FreeMemory, String> {
        let pointer =
            unsafe { (self.get_device_proc_addr)(device, b"vkFreeMemory\0".as_ptr().cast()) };
        if pointer.is_null() {
            return Err("vkFreeMemory is unavailable".to_owned());
        }
        Ok(unsafe { std::mem::transmute(pointer) })
    }
}

struct ExternalBuffer {
    vk_buffer: ash::vk::Buffer,
    vk_memory: ash::vk::DeviceMemory,
    vk_device: ash::vk::Device,
    memory_size: u64,
    fd: i32,
    free_memory: FreeMemory,
}

pub struct ZeroCopyAllocation {
    wgpu_buffer: Option<wgpu::Buffer>,
    mapped: Option<MappedBuffer>,
    vk_device: ash::vk::Device,
    vk_memory: ash::vk::DeviceMemory,
    free_memory: FreeMemory,
}

unsafe impl Send for ZeroCopyAllocation {}

impl Drop for ZeroCopyAllocation {
    fn drop(&mut self) {
        // Drop the wgpu buffer first so its Vulkan buffer is destroyed.
        self.wgpu_buffer.take();
        // Drop the CUDA mapping before releasing the Vulkan memory.
        self.mapped.take();
        unsafe {
            let _ = (self.free_memory)(self.vk_device, self.vk_memory, std::ptr::null());
        }
    }
}

pub struct ZeroCopyUpload {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    pub source: [u32; 2],
    pub allocation: ZeroCopyAllocation,
    pub decode_time: Duration,
    pub convert_time: Duration,
}

pub struct ZeroCopyConverter {
    device: Device,
    queue: Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    vulkan: VulkanFns,
}

impl ZeroCopyConverter {
    pub fn new(device: Device, queue: Queue) -> Result<Self, String> {
        let vulkan = VulkanFns::load()?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("zero-copy-rgb-to-rgba"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("zero-copy-bind-group-layout"),
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
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("zero-copy-pipeline-layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("zero-copy-pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            layout,
            vulkan,
        })
    }

    pub fn decode(
        &self,
        decoder: &mut NvJpegDecoder,
        path: &Path,
        target: [u32; 2],
    ) -> Result<ZeroCopyUpload, String> {
        let decode_started = Instant::now();
        let data = std::fs::read(path).map_err(|error| format!("read: {error}"))?;
        let plan = decoder.plan(&data, target)?;
        let output_width = plan.output[0];
        let output_height = plan.output[1];
        let byte_len = plan.pitch as u64 * output_height as u64;

        let external = self.create_buffer(byte_len)?;
        let file = unsafe { File::from_raw_fd(external.fd) };
        let cuda = decoder.cuda();
        let external_memory = unsafe { cuda.import_external_memory(file, external.memory_size) }
            .map_err(|error| format!("CUDA import: {error:?}"))?;
        let mapped = external_memory
            .map_all()
            .map_err(|error| format!("CUDA map: {error:?}"))?;
        let stream = decoder.cuda_stream().clone();
        let (pointer, guard) = mapped.device_ptr(&stream);
        decoder.decode_jpeg_into(&data, plan, pointer as *mut u8)?;
        drop(guard);
        stream
            .synchronize()
            .map_err(|error| format!("CUDA sync: {error:?}"))?;
        let decode_time = decode_started.elapsed();

        let hal_buffer = unsafe { wgpu_hal::vulkan::Device::buffer_from_raw(external.vk_buffer) };
        let wgpu_buffer = unsafe {
            self.device
                .create_buffer_from_hal::<wgpu_hal::api::Vulkan>(
                    hal_buffer,
                    &wgpu::BufferDescriptor {
                        label: Some("zero-copy-source"),
                        size: external.memory_size,
                        usage: wgpu::BufferUsages::STORAGE,
                        mapped_at_creation: false,
                    },
                )
        };

        let allocation = ZeroCopyAllocation {
            wgpu_buffer: Some(wgpu_buffer),
            mapped: Some(mapped),
            vk_device: external.vk_device,
            vk_memory: external.vk_memory,
            free_memory: external.free_memory,
        };

        let convert_started = Instant::now();
        let (texture, view) = self.convert(
            &allocation.wgpu_buffer.as_ref().unwrap(),
            output_width,
            output_height,
            plan.pitch as u32,
        )?;
        let convert_time = convert_started.elapsed();

        Ok(ZeroCopyUpload {
            texture,
            view,
            width: output_width,
            height: output_height,
            source: plan.source,
            allocation,
            decode_time,
            convert_time,
        })
    }

    fn convert(
        &self,
        source: &wgpu::Buffer,
        width: u32,
        height: u32,
        pitch: u32,
    ) -> Result<(wgpu::Texture, wgpu::TextureView), String> {
        let dst_pitch = ((width as usize * 4 + 255) & !255).max(256) as u32;
        let mut params = [0u8; 16];
        params[0..4].copy_from_slice(&width.to_le_bytes());
        params[4..8].copy_from_slice(&height.to_le_bytes());
        params[8..12].copy_from_slice(&pitch.to_le_bytes());
        params[12..16].copy_from_slice(&dst_pitch.to_le_bytes());
        let params_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("zero-copy-params"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&params_buffer, 0, &params);

        let byte_size = dst_pitch as u64 * height as u64;
        let destination = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("zero-copy-rgba"),
            size: byte_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zero-copy-bind-group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: source.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: destination.as_entire_binding(),
                },
            ],
        });

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("zero-copy-output"),
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

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("zero-copy-encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("zero-copy-pass"),
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
                    bytes_per_row: Some(dst_pitch),
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
        Ok((texture, view))
    }

    fn create_buffer(&self, size: u64) -> Result<ExternalBuffer, String> {
        let device = self.device.clone();
        let vulkan = &self.vulkan;
        let result = unsafe {
            device.as_hal::<wgpu_hal::api::Vulkan, _, _>(|hal| {
            let hal = hal?;
            let raw = hal.raw_device();
            let physical = hal.raw_physical_device();
            let queue_family = hal.queue_family_index();
            {
                let mut properties = ash::vk::PhysicalDeviceMemoryProperties::default();
                (vulkan.get_memory_properties)(physical, &mut properties);

                let mut external = ash::vk::ExternalMemoryBufferCreateInfo::default()
                    .handle_types(ash::vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
                let families = [queue_family, ash::vk::QUEUE_FAMILY_EXTERNAL];
                let info = ash::vk::BufferCreateInfo::default()
                    .size(size)
                    .usage(
                        ash::vk::BufferUsageFlags::STORAGE_BUFFER
                            | ash::vk::BufferUsageFlags::TRANSFER_SRC,
                    )
                    .sharing_mode(ash::vk::SharingMode::CONCURRENT)
                    .queue_family_indices(&families)
                    .push_next(&mut external);
                let buffer = match raw.create_buffer(&info, None) {
                    Ok(buffer) => buffer,
                    Err(error) => {
                        return Some(Err(format!("vkCreateBuffer: {error:?}")));
                    }
                };
                let requirements = raw.get_buffer_memory_requirements(buffer);
                let memory_type = (0..properties.memory_type_count).find(|index| {
                    requirements.memory_type_bits & (1 << index) != 0
                        && properties.memory_types[*index as usize]
                            .property_flags
                            .contains(ash::vk::MemoryPropertyFlags::DEVICE_LOCAL)
                });
                let Some(memory_type) = memory_type else {
                    raw.destroy_buffer(buffer, None);
                    return Some(Err("no device-local memory type".to_owned()));
                };
                let mut export = ash::vk::ExportMemoryAllocateInfo::default()
                    .handle_types(ash::vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
                let allocation = ash::vk::MemoryAllocateInfo::default()
                    .allocation_size(requirements.size)
                    .memory_type_index(memory_type)
                    .push_next(&mut export);
                let memory = match raw.allocate_memory(&allocation, None) {
                    Ok(memory) => memory,
                    Err(error) => {
                        raw.destroy_buffer(buffer, None);
                        return Some(Err(format!("vkAllocateMemory: {error:?}")));
                    }
                };
                if let Err(error) = raw.bind_buffer_memory(buffer, memory, 0) {
                    raw.free_memory(memory, None);
                    raw.destroy_buffer(buffer, None);
                    return Some(Err(format!("vkBindBufferMemory: {error:?}")));
                }
                let get_fd = match vulkan.memory_fd(raw.handle()) {
                    Ok(get_fd) => get_fd,
                    Err(error) => {
                        raw.free_memory(memory, None);
                        raw.destroy_buffer(buffer, None);
                        return Some(Err(error));
                    }
                };
                let free_memory = match vulkan.free_memory(raw.handle()) {
                    Ok(free_memory) => free_memory,
                    Err(error) => {
                        raw.free_memory(memory, None);
                        raw.destroy_buffer(buffer, None);
                        return Some(Err(error));
                    }
                };
                let mut fd = -1;
                let fd_info = ash::vk::MemoryGetFdInfoKHR::default()
                    .memory(memory)
                    .handle_type(ash::vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
                if get_fd(raw.handle(), &fd_info, &mut fd) != ash::vk::Result::SUCCESS {
                    raw.free_memory(memory, None);
                    raw.destroy_buffer(buffer, None);
                    return Some(Err("vkGetMemoryFdKHR failed".to_owned()));
                }
                Some(Ok(ExternalBuffer {
                    vk_buffer: buffer,
                    vk_memory: memory,
                    vk_device: raw.handle(),
                    memory_size: requirements.size,
                    fd,
                    free_memory,
                }))
            }
            })
        };

        match result {
            Some(Ok(buffer)) => Ok(buffer),
            Some(Err(error)) => Err(error),
            None => Err("wgpu device is not backed by Vulkan".to_owned()),
        }
    }
}

const SHADER: &str = r#"
struct Params {
    width: u32,
    height: u32,
    pitch: u32,
    destination_pitch: u32,
};
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> source: array<u32>;
@group(0) @binding(2) var<storage, read_write> destination: array<u32>;

fn byte_at(index: u32) -> u32 {
    let word = source[index / 4u];
    let shift = (index % 4u) * 8u;
    return (word >> shift) & 255u;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y >= params.height) {
        return;
    }
    let offset = id.y * params.pitch + id.x * 3u;
    let red = byte_at(offset);
    let green = byte_at(offset + 1u);
    let blue = byte_at(offset + 2u);
    destination[id.y * (params.destination_pitch / 4u) + id.x] =
        red | (green << 8u) | (blue << 16u) | (255u << 24u);
}
"#;

/// Box-average downsample of an RGBA texture into a smaller one. Keeps the
/// persistent viewer texture at the display size the stage needs, so a 24 MP
/// JPEG occupies a few MB instead of ~92 MB of device memory.
pub struct Downsampler {
    device: Device,
    queue: Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}

impl Downsampler {
    pub fn new(device: Device, queue: Queue) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("downsample-shader"),
            source: wgpu::ShaderSource::Wgsl(DOWNSAMPLE_SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("downsample-bind-group-layout"),
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
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("downsample-pipeline-layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("downsample-pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            device,
            queue,
            pipeline,
            layout,
        }
    }

    /// Renders a texture fitting inside `fit` (aspect preserved). Returns an
    /// error when the source already fits so the caller keeps the original.
    pub fn run(
        &self,
        view: &wgpu::TextureView,
        source: [u32; 2],
        fit: [u32; 2],
    ) -> Result<(wgpu::Texture, wgpu::TextureView, [u32; 2]), String> {
        let scale = (fit[0] as f32 / source[0] as f32)
            .min(fit[1] as f32 / source[1] as f32)
            .min(1.0);
        if scale >= 1.0 {
            return Err("no downsample needed".to_owned());
        }
        let width = ((source[0] as f32 * scale).floor() as u32).max(1);
        let height = ((source[1] as f32 * scale).floor() as u32).max(1);
        let pitch = ((width as usize * 4 + 255) & !255).max(256) as u32;

        let mut params = [0u8; 32];
        params[0..4].copy_from_slice(&source[0].to_le_bytes());
        params[4..8].copy_from_slice(&source[1].to_le_bytes());
        params[8..12].copy_from_slice(&width.to_le_bytes());
        params[12..16].copy_from_slice(&height.to_le_bytes());
        params[16..20].copy_from_slice(&pitch.to_le_bytes());
        let params_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("downsample-params"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&params_buffer, 0, &params);

        let destination = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("downsample-rgba"),
            size: pitch as u64 * height as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("downsample-bind-group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: destination.as_entire_binding(),
                },
            ],
        });
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("downsample-output"),
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
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("downsample-encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("downsample-pass"),
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
        Ok((texture, texture_view, [width, height]))
    }
}

const DOWNSAMPLE_SHADER: &str = r#"
struct DownParams {
    src: vec2<u32>,
    out: vec2<u32>,
    pitch: u32,
    pad: u32,
};
@group(0) @binding(0) var<uniform> params: DownParams;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var<storage, read_write> destination: array<u32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.out.x || id.y >= params.out.y) {
        return;
    }
    let ratio = vec2<f32>(params.src) / vec2<f32>(params.out);
    let taps = vec2<u32>(
        max(1u, u32(floor(ratio.x + 0.5))),
        max(1u, u32(floor(ratio.y + 0.5))),
    );
    let step = ratio / vec2<f32>(taps);
    let center = (vec2<f32>(id.xy) + vec2<f32>(0.5, 0.5)) * ratio - step * 0.5;
    var sum = vec4<f32>(0.0);
    for (var i = 0u; i < taps.x; i = i + 1u) {
        for (var j = 0u; j < taps.y; j = j + 1u) {
            let offset = vec2<f32>(f32(i), f32(j)) * step;
            let coords = clamp(round(center + offset), vec2<f32>(0.0), vec2<f32>(params.src) - vec2<f32>(1.0, 1.0));
            sum = sum + textureLoad(source, vec2<i32>(coords), 0);
        }
    }
    let average = sum / vec4<f32>(f32(taps.x * taps.y));
    let rgba = vec4<u32>(
        u32(round(clamp(average.r, 0.0, 1.0) * 255.0)),
        u32(round(clamp(average.g, 0.0, 1.0) * 255.0)),
        u32(round(clamp(average.b, 0.0, 1.0) * 255.0)),
        u32(round(clamp(average.a, 0.0, 1.0) * 255.0)),
    );
    destination[id.y * (params.pitch / 4u) + id.x] =
        rgba.r | (rgba.g << 8u) | (rgba.b << 16u) | (rgba.a << 24u);
}
"#;
