# Gallery

A fast, compact VS Code / Zed-style image gallery and editor built with Rust,
egui 0.37, and wgpu/Vulkan. The palette follows the in-tree shadcn zinc tokens.

## Features

Browse:

- shadcn/ui zinc light and dark themes, following the system color scheme on startup
- Folder tree in the sidebar with lazy expansion, plus Home / Pictures / Downloads / Desktop shortcuts
- Non-recursive and recursive folder scanning
- Virtualized thumbnail grid: only visible rows are decoded and drawn
- Thumbnail size dropdown (Small / Medium / Large), name / date / size sorting, and filename search
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

Edit (VS Code / Zed-style compact workspace):

- Custom GNOME-style header bar: app mark and app name, draggable title area,
  and minimize / maximize / close controls on the right. The window uses a
  rounded shell with transparent corners, and edge drag handles for resizing
- Compact, flush panels: a full-height left library sidebar, the central
  content, and the right editor panel sit edge to edge with 1 px separators.
  Only the window shell is rounded; there are no floating cards or inset gaps
- The 36 px toolbar keeps Open Folder, sidebar, and the right-side actions
  (Adjust / Image Info / open-in-file-manager / Export / theme / refresh).
  Rotate, flip, crop and copy-image live in the viewer toolbar
- Floating scrollbars fade in when the pointer enters a panel and expand when
  the bar is hovered; wheel scrolling keeps egui's smooth easing, with a
  100 px line step so it matches Chrome/GTK sensitivity
- Two-row library header: folder name and path above, then a wrapping control
  row with search, sort, recursive scan and the Grid size dropdown. Controls
  wrap instead of overlapping on narrow windows
- Flat collapsible sections with compact 28 px headers and separator lines.
  The right panel has a VS Code-style Adjust / Info tab strip, a Reset all
  action, and a user-resizable width (260–420 px)
- Stacked sliders: label and editable value share the first line, and the track
  uses the full panel width. Drag the track, Shift for fine control, click the
  value to type, double-click the track or click the label to reset.
  Temperature, tint, hue and HSL sliders use gradient rails
- A 24 px bottom status bar shows filename / folder, dimensions, file size and
  selection state on the left, with compact panel toggles and a zoom slider on
  the right
- Zoom and pan: wheel zooms at the cursor, drag pans, double-click toggles
  Fit ↔ 100%, with zoom out, a zoom slider, percentage, and Fit in the status
  bar; keys `+` `-` `0` `1`
- Right panel switches between **Adjust** and **Image Info** (path, dimensions,
  export size, megapixels, file size, format, created and modified times, edit
  summary)
- Crop tool: on-image overlay (dim outside, rule-of-thirds grid, corner grips,
  drag), aspect presets, free straightening, crop semantics handled in GPU
  geometry (shader.wgsl), Escape exits crop first
- Color grading in a single WGSL compute pass used by both the preview and the
  export worker: sRGB → linear, exposure, white balance (temperature + tint via
  Robertson + Bradford), contrast/highlights/shadows/whites/blacks, hue,
  saturation, vibrance, brightness with filmic-style mapping
- HSL mixer with eight hue ranges (red → magenta), each with hue / saturation /
  luminance, plus 4-wheel split toning (shadows / midtones / highlights /
  global) with master blending and balance
- Core sliders: exposure, brightness, contrast, highlights, shadows, whites,
  blacks, saturation, vibrance, hue, temperature, tint — packed into one 368 B
  `ColorParams` uniform, rewritten when an edit changes (not every frame)
- Per-image edits keyed on (path, base image, edit state); switching images
  keeps its own grade. Edits live in memory for the session.
- 3D LUT support: import `.cube` luts via a file dialog; they persist in
  `~/.cache/rust-gallery/luts` across launches, trilinear-sampled in the
  shader; the LUT section is the first section in the panel
- Export: full-resolution decode, the same grade compute pass offscreen, GPU
  readback, JPEG encode with a `.json` sidecar describing the recipe
- shadcn/ui design system implemented in-tree: zinc light/dark tokens, neutral
  primary controls, shadcn button/switch/slider/tab styling, and a data-driven
  Lucide icon table (no icon-font or shadcn crate dependency)

Supported formats: JPEG, PNG, WebP, GIF (first frame), BMP, TIFF.

## Build and run

```bash
cargo run --release
cargo run --release -- /path/to/photos
```

Requires Rust 1.95+ (a `rust-toolchain.toml` pins this project to a recent
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

Thumbnails are cached under `~/.cache/rust-gallery/thumbs`; imported LUTs live
under `~/.cache/rust-gallery/luts`.

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
| `Ctrl+B` | Toggle the file sidebar |
| `Ctrl+C` | Copy the current edited image to the clipboard |
| `←` `→` `↑` `↓` | Move grid selection; navigate in the viewer |
| `Home` / `End` | First / last image |
| `Enter` | Open the selected image |
| `E` | Toggle the adjust panel |
| `+` / `-` | Zoom in / out (viewer) |
| `0` / `1` | Fit to window / 100% (viewer) |
| `Esc` | Leave crop mode, then the adjust panel, then the viewer |
| `Esc` (no viewer) | Clear search and drop focus |

## Design

- `src/theme.rs` — system theme detection, bundled Inter fonts, egui `Visuals`/`Style` mapping
- `src/ui/tokens.rs` — compile-time design tokens: shadcn zinc light/dark palettes, metrics (radii, spacing, control sizes, panel widths)
- `src/ui/icons.rs` — data-driven Lucide icon geometry (`IconDef`/`StrokeDef`) and the renderer
- `src/ui/actions.rs` — declarative top-bar action lists and button styles
- `src/ui/layout.rs` — right-panel tabs and edit-section order
- `src/ui/edit_panel.rs` — right adjust panel sections, crop overlay
- `src/ui/info_panel.rs` — Image Info tab (path, dimensions, size, format, modified, edit summary)
- `src/ui/widgets.rs` — shadcn-styled buttons, icon buttons, switch, sliders, tabs/chips, color wheels
- `src/app.rs` — application state, layout, grid/viewer, zoom/pan, keyboard handling, toolbar
- `src/grade/mod.rs` — `EditState` (crop/aspect/orientation/adjustments/HSL/grading/LUT)
- `src/grade/gpu.rs` — `GradeCore`/`GradePass`: WGSL grading compute pipeline, uniforms, LUT registry
- `src/grade/shader.wgsl` — geometry + color pipeline, ported math from RapidRAW
- `src/grade/wb.rs` — Robertson white balance f64 math (Bradford chromatic adaptation)
- `src/crop.rs` — crop-rectangle math and straighten solve
- `src/export.rs` — full-resolution export worker: decode → offscreen grade → readback → JPEG + sidecar
- `src/reveal.rs` — open the image in the default file manager (FileManager1 → Nautilus → `gio open` → `xdg-open`)
- `src/luts.rs` — `.cube` parser, persistent LUT store, 3D texture upload
- `src/scan.rs` — background directory scanning
- `src/thumbs.rs` — thumbnail workers, disk cache, CPU image decoding
- `src/viewer.rs` — full-image decode workers, prefetching, LRU cache, GPU uploads
- `src/nvjpeg.rs` — dynamically loaded nvJPEG bindings and CUDA decoding
- `src/zerocopy.rs` — CUDA-imported Vulkan buffers and the RGB → RGBA compute pass

## Customizing the UI

The UI is data-driven; no widget rewrites are needed for the common changes:

- **Colors** — edit the `Palette::light()` / `Palette::dark()` presets in
  `src/ui/tokens.rs`. The fields map onto the shadcn roles (`background`,
  `foreground`, `muted`, `primary`, `border`, `ring`, …).
- **Sizes, radii, spacing** — edit `Metrics::default()` in `src/ui/tokens.rs`
  (panel/sidebar widths, bar heights, control heights, radii, gaps).
- **Icons** — edit the `StrokeDef` tables in `src/ui/icons.rs`; add an `Icon`
  variant and a `icon_def` arm to add a new one.
- **Top bar** — move an `ActionId` between `LEFT_ACTIONS`, `MIDDLE_ACTIONS` and
  `RIGHT_ACTIONS` in `src/ui/actions.rs`; change `ButtonStyle` to restyle it.
- **Right panel** — switch the tab list in `src/ui/layout.rs` or reorder
  `EDIT_SECTIONS` to rearrange the adjust panel.

Fonts are bundled Inter text faces in `assets/fonts` (OFL, see `LICENSE.txt`),
registered in `src/theme.rs`; text sizes live in the `style.text_styles` table
there. sub-pixel glyph binning is disabled for crisper text (egui renders
grayscale antialiasing only, so it cannot match GTK's LCD sub-pixel AA). Swap
the two `.ttf` files to change the UI font.

The shadcn Rust crates (`shadcn-egui`, `egui-shadcn`) currently target
egui 0.33/0.34, so this project implements the shadcn tokens and components
in-tree for egui 0.37 instead of downgrading the graphics stack.

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

- Zoom-aware full-resolution display: dedicated 100% viewing beyond the 4096 px
  decode cap
- GPU nvJPEG export encode: hand the graded pixels to nvJPEG from device memory
  (the export readback path is the current bottleneck) instead of CPU JPEG
- Re-import the sidecar to restore a grade on a previously exported image
- Live straighten preview inside crop mode (the preview shows the unrotated
  frame; the angle applies on exit)
- Thumbnail cache size limit and cleanup
- Remember last folder, window size, and theme between launches
- Optional `theme.toml` runtime overrides for the compile-time tokens
