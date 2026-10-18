# gallery-app

GNOME-style image gallery: Rust 1.85, egui/eframe 0.32, wgpu 25 (Vulkan first,
discrete GPU). Single binary crate, no workspace.

Global rules in `~/code/AGENTS.md` still apply; this file adds only project and
Rust specifics.

## Build and verify

- Iterate with `cargo check -q --message-format=short`. Never `cargo build` just
  to type-check.
- Lint gate: `cargo clippy -q --all-targets --message-format=short`. Add
  `-- -D warnings` only after the baseline is clean.
- Tests: `cargo test -q`. There is no test module yet; do not claim tests passed.
- Format: `cargo fmt` after edits; `cargo fmt --check` only when finishing.
- A silent `cargo check -q` is success. Do not re-run it to confirm.
- On failure, never dump the whole log:
  `cargo check > /tmp/opencode/cargo.log 2>&1 || tail -n 40 /tmp/opencode/cargo.log`
- Use `--keep-going` when several errors are likely, to avoid repeated runs.
- `--message-format=json` is more verbose, not less. Use it only when filtering
  fields, never as the default agent output.
- Cargo cannot set `message-format` in `.cargo/config.toml`; pass the flag.

## What not to read

- Never read `Cargo.lock`, `target/`, or `~/.cargo/registry` to answer a question.
- Dependencies: `cargo metadata --no-deps --format-version 1` for the package
  graph, `cargo tree --depth 1` for a shallow tree, `cargo tree -d` for duplicates.
- Do not `cargo clean` or delete `target/` unless asked; it is the time and token
  cache. Cold rebuilds are the most expensive output you can trigger.

## Editing

- Add/remove dependencies by editing `Cargo.toml`; let Cargo update `Cargo.lock`.
  Never hand-edit `Cargo.lock`.
- Prefer existing crates. Ask before adding a dependency or changing `image`
  format features.
- Keep the module layout; no extra abstraction for one-off UI code.
- `unwrap`/`expect` only for infallible invariants (e.g. a non-zero constant);
  runtime failures use `Result` + `log`.
- Match Adwaita styling in `theme.rs` / `ui/widgets.rs`, not egui defaults.

## Architecture

- `src/main.rs` — entry, wgpu adapter choice (Vulkan > GL, discrete GPU).
- `src/app.rs` (~1.4k lines) — state, layout, grid/viewer, keyboard, drag-drop.
  Never read it whole; locate the region with `rg`/semble, then read that range.
- `src/scan.rs` — background folder scan over `crossbeam-channel`.
- `src/thumbs.rs` — thumbnail workers, LRU texture cache, disk cache at
  `~/.cache/rust-gallery/thumbs`, full-image loader.
- `src/theme.rs` — Adwaita light/dark palettes, system theme, fonts.
- `src/ui/widgets.rs` — shared GNOME-style controls.
- `src/model.rs` — `ImageItem`, `SortKey`, `ThumbSize`, `is_image`.

## Invariants

- Never block egui's `update()`: scanning, decoding, and disk IO belong on worker
  threads reporting through channels.
- Thumbnails decode on workers and upload to the GPU once; pan/scroll/resize must
  not re-upload textures.
- Keep the UI responsive during a scan; stale results are discarded through
  `scan_generation`.

## Run and check the GUI

- Run only when GUI behaviour must be checked. Release builds are slow; prefer an
  existing binary, otherwise:
  `cargo run --release > /tmp/opencode/gallery.log 2>&1 &` then `tail -n 40`.
- Launch from a real desktop session; there is no headless path for the window.
- "Compiles" means `check`/`clippy` passed. It does not mean the feature was
  exercised. Report what you actually observed.

## Web research

- Use `chrome-devtools-axi run` with Google; `run` returns only stdout.
- Never use `open`/`snapshot` for research; they print the full accessibility tree.
- Extract 5-6 results (title + URL) with `page.eval`; never paste raw HTML.

```bash
CHROME_DEVTOOLS_AXI_AUTO_CONNECT=1 npx -y chrome-devtools-axi run <<'EOF'
await page.open("https://www.google.com/search?q=QUERY&hl=en");
await page.wait("#search");
console.log(JSON.stringify(await page.eval(() => [...document.querySelectorAll("#search h3")].slice(0, 6).map(h => ({ t: h.innerText, u: h.closest("a")?.href })))));
EOF
```

- Read only the chosen page:
  `page.eval(() => (document.querySelector("article")?.innerText || document.body.innerText).slice(0, 4000))`
- Fall back to `websearch` if Google shows consent/captcha.
- For a known doc URL, `webfetch`/`curl` is cheaper than a browser.

## Semantic search

Scope `semble.search` to `/home/loq/code/rust/gallery-app`, `top_k` 3-5,
`max_snippet_lines` 10. Never index `target/`.
