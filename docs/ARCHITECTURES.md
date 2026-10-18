# Architecture and fast paths

This document explains how Gallery stays responsive while browsing large
folders, decoding full-resolution images, grading on the GPU and exporting.
The short version: the UI thread only paints and dispatches; every expensive
operation runs on a worker, and decoded pixels stay on the GPU whenever
possible.

See also the module list in the main [README](../README.md#design).

## Goals

- Never block the egui frame on disk I/O, image decode or GPU readback.
- Make grid scrolling independent of folder size.
- Make image navigation feel instant by caching and prefetching.
- Share one color/geometry implementation between preview and export.
- Use hardware decoding and zero-copy paths when the driver supports them,
  with correct CPU/OpenGL fallbacks.

## Thread and channel map

| Thread | Responsibility | Reports via |
| --- | --- | --- |
| UI (`eframe`) | egui input/paint, panel layout, polling results | — |
| Scan | `read_dir` or recursive `WalkDir`, metadata, sorting | bounded `ScanOutcome` |
| Thumbnail workers (2–8) | decode 512 px thumbnails, write disk cache | unbounded `ThumbResult` |
| Viewer current | decode the requested image to a GPU texture | unbounded `LoadResult` |
| Viewer prefetch | decode the next likely image | unbounded `LoadResult` |
| Export | full-res decode, offscreen grade, readback, JPEG + sidecar | one `ExportOutcome` |
| Clipboard copy | full-res decode, offscreen grade, RGBA readback | one `CopyOutcome` |
| Reveal | file-manager call | completion channel |

```text
             ┌──────────────────────────┐
 input ─────▶│ UI thread (egui + wgpu)  │──────▶ screen
             └───────┬─────────▲────────┘
                     │         │ poll channels
        ┌────────────┘         └───────────────┐
        ▼                                      │
  scan worker ──▶ filtered/sorted image list   │
        │                                      │
  thumbnail workers ──▶ disk cache + LRU ──────┤
        │                                      │
  viewer current/prefetch ──▶ GPU textures ────┤
        │                                      │
  export / clipboard workers ──▶ files/clipboard
```

Stale work is discarded rather than awaited:

- scans carry a generation number; older generations are ignored
- viewer slots keep only the newest request per worker
- texture uploads for a request that is no longer active are still cached, but
  never block the current frame

## Browse pipeline

1. `scan.rs` walks the selected folder on a worker. Recursive scans skip hidden
   directories; flat scans use `read_dir`.
2. Each image becomes an `ImageItem` with path, display name, size and
   timestamps. Search and sorting are applied in memory.
3. The grid is virtualized with `ScrollArea::show_rows`. Only visible rows are
   laid out and painted; a thumbnail is requested only for a visible cell.
4. `thumbs.rs` decodes at most 512 px, writes a persistent JPEG cache under
   `~/.cache/rust-gallery/thumbs`, and keeps up to 350 textures in memory.
5. Grid selection uses a computed row/column rect, so keyboard navigation can
   scroll to an item even when it is outside the currently rendered row range.

## Viewer pipeline

1. The grid already has a thumbnail. Opening an image draws it immediately.
2. `viewer_target` maps the stage size and zoom to a decode target in 128 px
   steps, clamped to 512–4096 px. Zoom steps change the target only at coarse
   thresholds (1.5×, 3×, 6×), so wheel zooming does not queue a decode per tick.
3. The current and prefetch workers decode with nvJPEG when available, otherwise
   the CPU path. The result is uploaded once to a wgpu texture.
4. The viewer keeps an LRU of 32 textures / 1.2 GB. A texture large enough for
   the current target can be reused while a higher-resolution decode is in
   flight.
5. Edits are applied as a separate GPU pass. The viewer keeps the last graded
   texture keyed by `(path, base texture, base size, edit state)` and recomputes
   it only when one of those changes.

## GPU grade pipeline

`grade/gpu.rs` builds one compute pipeline shared by preview and export.
`grade/shader.wgsl` performs, in one dispatch:

- orientation steps and horizontal/vertical flips
- free rotation and crop geometry
- sRGB ↔ linear conversion
- exposure, white balance via Robertson + Bradford, contrast, highlights,
  shadows, whites and blacks
- hue, saturation, vibrance, brightness and filmic mapping
- eight HSL ranges
- four-way split toning with blending and balance
- optional trilinear 3D LUT sampling

The parameters fit in a 368 B uniform. It is rewritten only when the edit
changes, not every frame. The same `GradeCore` is used by the preview pass and
by the offscreen export pass, so preview and output cannot drift apart.

## Zero-copy JPEG path

When the NVIDIA driver exposes the required external-memory handles:

```text
JPEG file
  │
  ▼
nvJPEG (CUDA) ──▶ CUDA device memory
  │
  ▼
CUDA-imported Vulkan buffer
  │
  ▼
wgpu storage buffer ──▶ RGB→RGBA compute pass ──▶ display texture
  │
  ▼
egui samples the texture
```

No decoded pixels are copied through the CPU or across PCIe after decoding.
If nvJPEG or the import path is unavailable, the app falls back to nvJPEG with
a host readback, then to the CPU Rust `image` decoder.

## Export and clipboard

Both paths use `export::render_rgba`:

1. decode the source at full resolution
2. if the edit is non-identity, upload it and run the same `GradeCore` pipeline
3. read the result back
4. export: drop alpha, encode JPEG at quality 92 and write a `.json` sidecar
5. clipboard: convert the RGBA buffer to `egui::ColorImage` and call
   `Context::copy_image`

Both run on a worker thread. The UI shows progress/completion through toasts
and remains interactive.

## Caching and memory

| Cache | Limit | Purpose |
| --- | --- | --- |
| Thumbnail textures | 350 entries | fast grid scrolling |
| Thumbnail disk cache | unbounded | survive restarts |
| Viewer textures | 1.2 GB / 32 entries | instant back/forward navigation |
| Graded preview | last result | avoid re-grading unchanged edits |
| LUT textures | imported set | no per-frame LUT upload |

The viewer evicts least-recently-used textures when the byte budget is
exceeded, while keeping the active target.

## Rules that keep it fast

1. **Do not do I/O or decode in the UI thread.** Use a worker and a channel.
2. **Virtualize lists.** Draw only the visible rows.
3. **Cache decoded data.** Thumbnails on disk, textures on the GPU.
4. **Prefetch the next likely item.** Arrow navigation should not wait.
5. **Quantize decode targets.** Avoid a new decode every frame while zooming.
6. **Keep textures on the GPU.** Preview should not read pixels back to the CPU.
7. **Use one shader for preview and export.** One implementation, same result.
8. **Discard stale work.** A superseded scan/decode must not overwrite newer
   state.
9. **Write uniforms only on change.** Editing state, not frame time, drives the
   update.
10. **Use the fastest available decode/upload path.** nvJPEG + zero-copy when
    possible, with CPU/OpenGL fallbacks otherwise.

## Measured performance

On the tested laptop (RTX 2050, release build):

- 1.2 MP JPEG: about 10 ms nvJPEG decode + 10 ms GPU conversion
- 24 MP JPEG (6000×4000): about 75 ms decode + 4 ms GPU conversion

The nvJPEG/CUDA context is initialized in a worker at startup so the first
image open does not pay for context creation on the UI thread.

These numbers are hardware- and driver-dependent. The architectural rules
above matter more than any single benchmark.

## Known limits

- Preview decoding is capped at 4096 px; 100% inspection beyond that needs the
  full-resolution viewer path on the roadmap.
- The viewer cache is memory-budgeted, not disk-backed.
- Edits are session-only until sidecar re-import is implemented.
- The zero-copy path is NVIDIA-specific. Intel and AMD use Vulkan/OpenGL with
  the normal upload path.
- CPU/OpenGL fallbacks are correct but slower than the discrete-GPU Vulkan
  path.
