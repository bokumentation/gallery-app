mod app;
mod crop;
mod export;
mod grade;
mod model;
mod nvjpeg;
mod luts;
mod scan;
mod theme;
mod thumbs;
mod ui;
mod viewer;
mod zerocopy;

use std::sync::Arc;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,gallery_app=info"),
    )
    .init();

    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN | wgpu::Backends::GL;
    setup.power_preference = wgpu::PowerPreference::HighPerformance;
    setup.native_adapter_selector = Some(Arc::new(|adapters, _surface| {
        for adapter in adapters {
            let info = adapter.get_info();
            log::info!(
                "adapter: {} ({:?}, {:?}, vendor 0x{:04x})",
                info.name,
                info.backend,
                info.device_type,
                info.vendor
            );
        }
        adapters
            .iter()
            .max_by_key(|adapter| {
                let info = adapter.get_info();
                let backend = if info.backend == wgpu::Backend::Vulkan {
                    2
                } else {
                    0
                };
                let kind = match info.device_type {
                    wgpu::DeviceType::DiscreteGpu => 4,
                    wgpu::DeviceType::IntegratedGpu => 3,
                    wgpu::DeviceType::VirtualGpu => 2,
                    wgpu::DeviceType::Cpu => 1,
                    wgpu::DeviceType::Other => 0,
                };
                // Prefer NVIDIA when everything else is equal.
                let vendor = if info.vendor == 0x10de { 1 } else { 0 };
                (backend, kind, vendor)
            })
            .cloned()
            .ok_or_else(|| "no Vulkan or OpenGL adapter available".to_owned())
    }));

    let wgpu_options = egui_wgpu::WgpuConfiguration {
        surface: egui_wgpu::SurfaceConfig::LOW_LATENCY,
        wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup),
        ..Default::default()
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([880.0, 560.0])
            .with_title("Gallery"),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options,
        centered: true,
        // egui feather-antialiases geometry already; 4x MSAA additionally
        // smooths the image quads and costs little on this GPU.
        multisampling: 4,
        ..Default::default()
    };

    let start_dir = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| dirs::picture_dir().filter(|path| path.is_dir()))
        .or_else(dirs::home_dir);

    eframe::run_native(
        "Gallery",
        options,
        Box::new(move |cc| Ok(Box::new(app::GalleryApp::new(cc, start_dir)))),
    )
}
