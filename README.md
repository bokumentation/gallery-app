# Gallery

A fast, GNOME-style image gallery and editor built with Rust, egui 0.37, and
wgpu/Vulkan.

## Features

Browse:

- GNOME/Adwaita-inspired light and dark themes, following the system color scheme on startup
- Folder tree in the sidebar with lazy expansion, plus Home / Pictures / Downloads / Desktop shortcuts
- Non-recursive and recursive folder scanning
- Virtualized thumbnail grid: only visible rows are decoded and drawn
- Thumbnail sizes S / M / L, name / date / size sorting, and filename search
- Double-click or Enter opens an image in the main-area viewer
- `←` / `→` navigate images, `Esc` returns to the grid
- Instant thumbnail preview while the full-resolution image is still decoding
- Neighbor prefetching and a byte-budgeted full-image cache, so arrow navigation stays responsive
- Hardware JPEG decoding with nvJPEG/CUDA when available
- CUDA → Vulkan zero-copy JPEG path when the driver supports external memory: nvJPEG writes into imported Vulkan memory, a wgpu compute pass converts RGB to RGBA, and egui samples the result directly
- Drag a folder (or an image) onto the window to open it
- GPU rendering through wgpu's Vulkan backend, preferring the discrete GPU
- Background thumbnail workers, an in-memory LRU texture cache, and a persistent JPEG thumbnail cache
- EXIF orientation is applied when decoding

Edit (right-side adjust panel, RapidRAW-style layout):

- Top toolbar with rotate ⟲/⟳ 90° steps, flip H/V, crop toggle, adjust toggle,
  and an accent Export button (export itself is Phase 6); bottom bar with
  filename, dimensions and file size
- Crop tool: on-image overlay (dim outside, rule-of-thirds grid, corner grips,
  drag), aspect presets, free straightening, crop semantics handled in GPU
  geometry (shader.wgsl), Escape exits crop first
- Color grading in a single WGSL pass shared by preview and export: sRGB →
  linear, exposure, white balance (temperature + tint via Robertson +
  Bradford), contrast/highlights/shadows/whites/blacks, hue, saturation,
  vibrance, brightness with filmic-style mapping
- Core sliders: exposure, brightness, contrast, highlights, shadows, whites,
  blacks, saturation, vibrance, hue, temperature, tint — batched as one 160 B
  uniform per frame on the GPU
- Per-image edits keyed on (path, base image, edit state); switching images
  keeps its own grade
- 3D LUT support: import `.cube` luts via a file dialog; they persist in
  `~/.cache/rust-gallery/luts` across launches, trilinear-sampled in the
  shader; the LUT section is the first section in the panel
- Shadcn-aesthetic widgets: ghost/primary icon buttons with Lucide-style
  painted icons, switches, sliders, combobox

Supported formats: JPEG, PNG, WebP, GIF (first frame), BMP, TIFF.

## Build and run

```bash
cargo run --release
cargo run --release -- /path/to/photos
```

Requires Rust 1.96+ (a `rust-toolchain.toml` pins this project to a recent
stable; the system default toolchain is untouched) and a working Vulkan
driver. The folder picker uses the desktop portal, which is available by
default on GNOME.

For CUDA/nvJPEG acceleration, an NVIDIA driver and the nvJPEG runtime are
needed. Install it system-wide:

```bash
sudo apt install libnvjpeg-13-1 libnvjpeg-dev-13-1
```

or without sudo, into `~/.local/lib/rust-gallery`:

```bash
./scripts/install-nvjpeg-local.sh
```

The app probes nvJPEG and the Vulkan external-memory path at runtime. If either
is unavailable it falls back to the CUDA/host path, then to CPU decoding.

Environment variables:

| Variable | Effect |
| --- | --- |
| `NVJPEG_LIBRARY` | Explicit path to `libnvjpeg.so.13` |
| `GALLERY_ZERO_COPY=0` | Disable the CUDA → Vulkan zero-copy path |

Thumbnails are cached under `~/.cache/rust-gallery/thumbs`.

## Performance notes

On the RTX 2050 test machine (release build), zero-copy JPEG loads measured about
10 ms decode + 10 ms GPU conversion for a 1.2 MP image, and about 75 ms + 4 ms
for a 24 MP (6000×4000) image. The first open also initializes the CUDA/nvJPEG
context in a worker thread at app startup, so it does not block the UI.

## Keyboard

| Key | Action |
| --- | --- |
| `Ctrl+O` | Open folder |
| `Ctrl+F` | Focus search |
| `Esc` | Clear search / leave viewer |
| `←` `→` `↑` `↓` | Move grid selection; navigate in the viewer |
| `Home` / `End` | First / last image |
| `Enter` | Open the selected image |
| `E` | Toggle the adjust panel |
| `Esc` | Leave crop mode first, then leave the viewer |

## Design

- `src/theme.rs` — Adwaita light/dark palettes, system theme detection, bundled Inter fonts, ppi snapping
- `src/app.rs` — application state, layout, grid/viewer, keyboard handling, toolbar
- `src/ui/edit_panel.rs` — right adjust panel (LUT / crop / grading sections), crop overlay
- `src/ui/widgets.rs` — shadcn-aesthetic buttons/icons, switch, segmented control, search field
- `src/grade/mod.rs` — `EditState` (crop/aspect/orientation/adjustments/LUT)
- `src/grade/gpu.rs` — GradePass: WGSL grading compute pipeline, uniforms, LUT texture registry
- `src/grade/shader.wgsl` — color pipeline + LSB/geometry, ported math from RapidRAW
- `src/grade/wb.rs` — Robertson white balance f64 math (Bradford chromatic adaptation)
- `src/crop.rs` — crop-rectangle math and straighten solve
- `src/luts.rs` — `.cube` parser, persistent LUT store, 3D texture upload
- `src/scan.rs` — background directory scanning
- `src/thumbs.rs` — thumbnail workers, disk cache, CPU image decoding
- `src/viewer.rs` — full-image decode workers, prefetching, LRU cache, GPU uploads
- `src/nvjpeg.rs` — dynamically loaded nvJPEG bindings and CUDA decoding
- `src/zerocopy.rs` — CUDA-imported Vulkan buffers and the RGB → RGBA compute pass

Rendering is GPU-accelerated through `eframe`'s wgpu renderer. It prefers Vulkan
on the discrete GPU and falls back to OpenGL only if Vulkan is unavailable.

Viewer navigation is handled by two persistent workers (current image and
prefetch). New requests supersede queued ones, stale results are discarded, and
the viewer immediately shows the cached thumbnail while the full image loads.
Decoded images are cached as persistent GPU textures with a byte budget, so
moving back to a previously viewed image is instant.

When the zero-copy path is active, nvJPEG writes RGB data into a Vulkan buffer
whose memory was imported into CUDA. The buffer is wrapped as a wgpu storage
buffer, a compute shader expands RGB to RGBA, and the result is copied into the
display texture. No image pixels cross PCIe or the CPU after decoding.

## Roadmap

- HSL color mixer (8 color ranges) and 4-wheel split toning in the grade shader
- Export with a GPU-resident pipeline: full-resolution decode → WGSL geometry + grade →
  nvJPEG GPU encode from device memory (JPEG never crosses to the host), plus a per-image
  sidecar JSON storing the edit recipe (Export button wiring lands here)
- Full-resolution display textures: dedicated 100% viewing
- Thumbnail cache size limit and cleanup
- Remember last folder, window size, and theme between launches
- Zoom/pan in the viewer
