# Roadmap

Priorities are ordered by user-visible impact. Items are intentionally small
enough to ship independently.

## Performance

- **Full-resolution 100% viewing** — remove the 4096 px preview cap for
  inspection. Keep the current quantized decode path for fit/normal zoom and
  request an exact-resolution texture only at 100% or above.
- **GPU nvJPEG export** — feed the graded device buffer to nvJPEG directly
  instead of reading RGBA back to the CPU and encoding there. The readback is
  the current export bottleneck.
- **Cache tuning** — measure the 1.2 GB viewer budget and 350-thumbnail LRU on
  8/16 GB machines; expose a setting if the defaults are too aggressive.
- **Thumbnail cache cleanup** — add an age/size limit and a command to clear
  the persistent cache.

## Editing

- **Sidecar re-import** — read the exported `.json` recipe and restore the
  grade, crop, orientation and LUT selection when opening an edited image.
- **Live straighten preview** — apply the straighten angle inside crop mode;
  today the preview shows the unrotated frame and the angle applies on exit.
- **More export formats** — consider PNG/WebP output for images with alpha or
  lossless needs.
- **Persistent edit state** — optionally restore session edits after restart,
  with sidecars as the source of truth.

## Library and UX

- **Remember session state** — last folder, window size, right-panel width,
  theme and grid size between launches.
- **Optional `theme.toml`** — runtime overrides for the compile-time design
  tokens without rebuilding.
- **Folder search** — extend the current image search to match folder names in
  the sidebar.
- **Multi-select and batch actions** — copy paths, export or reveal several
  images at once.
- **Better empty/error states** — show scan progress and permission failures
  inline instead of only logging them.

## Platform

- Test on Fedora, Arch and Ubuntu with Intel, AMD and NVIDIA GPUs.
- Verify the zero-copy path across NVIDIA driver versions and Vulkan external
  memory implementations.
- Check HiDPI scaling and multi-monitor behavior on Wayland and X11.
- Confirm clipboard image copy on compositors without the data-control
  protocol (currently arboard falls back to X11/XWayland).

## How to contribute a roadmap item

1. Add a focused benchmark or reproduction first.
2. Keep expensive work on a worker; never block the egui frame.
3. Reuse `GradeCore` for preview and export paths.
4. Add a regression test where the behavior is testable without a display.
5. Update this file and the relevant section of
   [ARCHITECTURES.md](ARCHITECTURES.md) when the design changes.
