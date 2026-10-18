use crate::grade::{gpu::GradePass, EditState};
use crate::model::{is_image, ImageItem, SortKey, ThumbSize};
use crate::scan::{spawn_scan, ScanOutcome};
use crate::theme::{self, Palette, ThemeMode};
use crate::thumbs::{human_size, ThumbnailStore};
use crate::viewer::ViewerService;
use crate::ui::actions::{ActionId, ActionSpec, ButtonStyle, Visibility};
use crate::ui::layout::RightTab;
use crate::ui::widgets as w;
use crossbeam_channel::Receiver;
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, Rect, RichText, Sense,
    Stroke, StrokeKind, TextureId, Ui, Vec2,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const TOP_BAR_HEIGHT: f32 = 52.0;
const MAX_ZOOM: f32 = 8.0;

pub struct GalleryApp {
    pub(crate) theme_mode: ThemeMode,
    pub(crate) palette: Palette,
    pub(crate) show_sidebar: bool,
    places: Vec<Place>,
    root: FolderNode,
    expanded: HashSet<PathBuf>,
    selected_folder: Option<PathBuf>,
    current_dir: Option<PathBuf>,
    all_items: Vec<ImageItem>,
    visible: Vec<usize>,
    search: String,
    sort: SortKey,
    sort_desc: bool,
    thumb_size: ThumbSize,
    recursive: bool,
    selection: Option<usize>,
    grid_columns: usize,
    scroll_to_selection: bool,
    viewer_open: bool,
    viewer_index: usize,
    /// `None` = fit to window; `Some(scale)` = absolute image-pixel-to-point scale.
    pub(crate) zoom: Option<f32>,
    pub(crate) pan: Vec2,
    /// Fit scale from the last frame, used by keyboard zooming.
    pub(crate) viewer_fit: f32,
    /// Last texture actually drawn, kept while a zoom-triggered re-decode is in
    /// flight so the preview does not flash back to the thumbnail.
    viewer_display: Option<(PathBuf, TextureId)>,
    scan_generation: u64,
    scan_rx: Option<Receiver<ScanOutcome>>,
    scanning: bool,
    scan_elapsed: Option<Duration>,
    pub(crate) thumbs: ThumbnailStore,
    pub(crate) viewer: ViewerService,
    pub(crate) edit_panel_open: bool,
    pub(crate) right_tab: RightTab,
    pub(crate) crop_active: bool,
    pub(crate) crop_drag: Option<crate::crop::CropDrag>,
    pub(crate) lut_names: Vec<String>,
    /// Selected hue range in the HSL mixer (index into `grade::HSL_NAMES`).
    pub(crate) hsl_range: usize,
    pub(crate) edits: HashMap<PathBuf, EditState>,
    pub(crate) grade: GradePass,
    export_rx: Option<Receiver<crate::export::ExportOutcome>>,
    exporting: bool,
    reveal_rx: Option<Receiver<Result<(), String>>>,
    toast: Option<Toast>,
    pending_open: Option<PathBuf>,
    last_search: String,
    last_sort: SortKey,
    last_sort_desc: bool,
}

#[derive(Clone)]
struct Place {
    name: &'static str,
    path: PathBuf,
}

struct Toast {
    message: String,
    error: bool,
    expires: Instant,
}

struct FolderNode {
    path: PathBuf,
    name: String,
    children: Option<Vec<FolderNode>>,
    has_subdirs: bool,
}

impl FolderNode {
    fn new(path: PathBuf) -> Self {
        let name = display_name(&path);
        let has_subdirs = has_subdirs(&path);
        Self {
            path,
            name,
            children: None,
            has_subdirs,
        }
    }

    fn ensure_loaded(&mut self) {
        if self.children.is_some() {
            return;
        }
        let mut children = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                children.push(FolderNode::new(path));
            }
        }
        children.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.has_subdirs = !children.is_empty();
        self.children = Some(children);
    }
}

fn display_name(path: &Path) -> String {
    if let Some(name) = path.file_name() {
        return name.to_string_lossy().into_owned();
    }
    path.display().to_string()
}

fn has_subdirs(path: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        entry.path().is_dir() && !name.starts_with('.')
    })
}

impl GalleryApp {
    pub fn new(cc: &eframe::CreationContext<'_>, start_dir: Option<PathBuf>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        // Snap to a quarter-point grid: fractional scale factors (1.25, 1.5,
        // 1.75) stay exact but float noise like 1.2500001 stops leaking into
        // the font atlas, which keeps glyph rendering crisp.
        let native_ppp = cc.egui_ctx.native_pixels_per_point().unwrap_or(1.0);
        if native_ppp > 0.0 {
            cc.egui_ctx
                .set_pixels_per_point((native_ppp * 4.0).round() / 4.0);
        }
        let theme_mode = ThemeMode::System;
        let palette = Palette::from_mode(theme_mode);
        theme::apply(&cc.egui_ctx, palette);

        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let mut app = Self {
            theme_mode,
            palette,
            show_sidebar: true,
            places: collect_places(),
            root: FolderNode::new(home.clone()),
            expanded: HashSet::new(),
            selected_folder: None,
            current_dir: None,
            all_items: Vec::new(),
            visible: Vec::new(),
            search: String::new(),
            sort: SortKey::Name,
            sort_desc: false,
            thumb_size: ThumbSize::Medium,
            recursive: false,
            selection: None,
            grid_columns: 4,
            scroll_to_selection: false,
            viewer_open: false,
            viewer_index: 0,
            zoom: None,
            pan: Vec2::ZERO,
            viewer_fit: 1.0,
            viewer_display: None,
            scan_generation: 0,
            scan_rx: None,
            scanning: false,
            scan_elapsed: None,
            thumbs: ThumbnailStore::new(),
            viewer: ViewerService::new(
                cc.wgpu_render_state
                    .as_ref()
                    .expect("wgpu render state is available with the wgpu renderer"),
            ),
            edit_panel_open: false,
            right_tab: RightTab::Edit,
            crop_active: false,
            crop_drag: None,
            lut_names: Vec::new(),
            hsl_range: 0,
            edits: HashMap::new(),
            grade: GradePass::new(
                cc.wgpu_render_state
                    .as_ref()
                    .expect("wgpu render state is available with the wgpu renderer"),
            ),
            export_rx: None,
            exporting: false,
            reveal_rx: None,
            toast: None,
            pending_open: None,
            last_search: String::new(),
            last_sort: SortKey::Name,
            last_sort_desc: false,
        };

        // Persistent LUTs: parse every stored .cube into the grade pass once.
        for name in crate::luts::CubeLut::list() {
            if let Some(cube) = crate::luts::CubeLut::load(&name) {
                app.grade.import_lut(&cube);
                app.lut_names.push(name);
            }
        }

        app.expanded.insert(home.clone());
        app.navigate(start_dir.unwrap_or(home));
        app
    }

    fn navigate(&mut self, path: PathBuf) {
        if !path.is_dir() {
            self.show_toast(format!("Not a folder: {}", path.display()), true);
            return;
        }

        if !path.starts_with(&self.root.path) {
            self.root = FolderNode::new(path.clone());
            self.expanded.clear();
        }
        self.expand_to(&path);
        self.selected_folder = Some(path.clone());
        self.current_dir = Some(path.clone());
        self.selection = None;
        self.close_viewer();
        self.all_items.clear();
        self.visible.clear();
        self.pending_open = None;
        self.scan_generation += 1;
        self.scanning = true;
        self.scan_elapsed = None;
        self.scan_rx = Some(spawn_scan(
            path,
            self.recursive,
            self.scan_generation,
        ));
    }

    fn expand_to(&mut self, path: &Path) {
        if !path.starts_with(&self.root.path) {
            return;
        }
        let mut current = self.root.path.clone();
        self.expanded.insert(current.clone());
        if let Ok(relative) = path.strip_prefix(&self.root.path) {
            for component in relative.components() {
                current.push(component);
                self.expanded.insert(current.clone());
            }
        }
    }

    fn rescan(&mut self) {
        if let Some(dir) = self.current_dir.clone() {
            self.navigate(dir);
        }
    }

    /// Import a `.cube` LUT: parse it, copy it into the persistent store and
    /// upload it to the grade pass so it is available after restarts too.
    pub(crate) fn import_lut(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Cube LUT", &["cube"])
            .pick_file()
        else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            self.show_toast(format!("Cannot read {}", path.display()), true);
            return;
        };
        let Some(name) = path.file_stem().map(|stem| stem.to_string_lossy().into_owned())
        else {
            return;
        };
        let Some(cube) = crate::luts::CubeLut::parse(name, &text) else {
            self.show_toast("Not a valid 3D .cube LUT".to_owned(), true);
            return;
        };
        if let Err(error) = cube.save() {
            log::warn!("lut store failed: {error}");
        }
        if !self.lut_names.contains(&cube.name) {
            self.lut_names.push(cube.name.clone());
            self.lut_names.sort();
        }
        self.grade.import_lut(&cube);
        // Attach to the image being edited.
        if let Some(path) = self.current_viewer_path() {
            self.edits.entry(path).or_default().lut = Some(cube.name.clone());
        }
        self.show_toast(format!("Imported LUT: {}", cube.name), false);
    }

    /// Startup-time loading of the persisted LUT list (done in `new`); kept
    fn pick_folder(&mut self) {        log::info!("folder picker: opening");
        let mut dialog = rfd::FileDialog::new();
        if let Some(dir) = &self.current_dir {
            dialog = dialog.set_directory(dir);
        }
        let picked = dialog.pick_folder();
        log::info!("folder picker: {:?}", picked);
        if let Some(folder) = picked {
            self.navigate(folder);
        }
    }

    fn poll_scan(&mut self) {
        let Some(rx) = &self.scan_rx else {
            return;
        };
        let outcome = rx.try_recv().ok();
        let Some(outcome) = outcome else {
            return;
        };
        if outcome.generation != self.scan_generation {
            return;
        }
        self.scan_rx = None;
        self.scanning = false;
        self.scan_elapsed = Some(outcome.elapsed);
        if let Some(error) = outcome.error {
            self.show_toast(error, true);
        }
        self.all_items = outcome.items;
        self.rebuild_visible();
        if let Some(path) = self.pending_open.take() {
            if let Some(index) = self
                .visible
                .iter()
                .position(|&index| self.all_items[index].path == path)
            {
                self.open_viewer(index);
            }
        }
        log::info!(
            "scanned {} in {:?}: {} images",
            self.current_dir
                .as_deref()
                .unwrap_or(Path::new("?"))
                .display(),
            outcome.elapsed,
            self.all_items.len()
        );
    }

    fn sync_filters(&mut self) {
        if self.search == self.last_search
            && self.sort == self.last_sort
            && self.sort_desc == self.last_sort_desc
        {
            return;
        }
        self.last_search = self.search.clone();
        self.last_sort = self.sort;
        self.last_sort_desc = self.sort_desc;
        self.rebuild_visible();
    }

    fn rebuild_visible(&mut self) {
        let query = self.search.trim().to_lowercase();
        let mut indices: Vec<usize> = self
            .all_items
            .iter()
            .enumerate()
            .filter(|(_, item)| query.is_empty() || item.name.to_lowercase().contains(&query))
            .map(|(index, _)| index)
            .collect();

        let items = &self.all_items;
        indices.sort_by(|&a, &b| {
            let left = &items[a];
            let right = &items[b];
            let ordering = match self.sort {
                SortKey::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
                SortKey::Modified => left.modified.cmp(&right.modified),
                SortKey::Size => left.size.cmp(&right.size),
            };
            let ordering = ordering.then_with(|| left.path.cmp(&right.path));
            if self.sort_desc {
                ordering.reverse()
            } else {
                ordering
            }
        });

        self.visible = indices;
        if let Some(selection) = self.selection {
            if selection >= self.visible.len() {
                self.selection = self.visible.len().checked_sub(1);
            }
        }
        if self.viewer_open && self.viewer_index >= self.visible.len() {
            self.close_viewer();
        }
    }

    fn open_viewer(&mut self, index: usize) {
        if index >= self.visible.len() {
            return;
        }
        self.selection = Some(index);
        self.viewer_index = index;
        self.viewer_open = true;
        self.reset_zoom();
    }

    fn close_viewer(&mut self) {
        self.viewer_open = false;
        self.viewer.deactivate();
        self.grade.release();
        self.reset_zoom();
    }

    fn reset_zoom(&mut self) {
        self.zoom = None;
        self.pan = Vec2::ZERO;
        self.viewer_display = None;
    }

    /// Path of the image currently shown in the viewer.
    pub(crate) fn current_viewer_path(&self) -> Option<PathBuf> {
        self.visible
            .get(self.viewer_index)
            .and_then(|index| self.all_items.get(*index))
            .map(|item| item.path.clone())
    }

    /// The image currently shown in the viewer.
    pub(crate) fn current_item(&self) -> Option<ImageItem> {
        self.visible
            .get(self.viewer_index)
            .and_then(|index| self.all_items.get(*index))
            .cloned()
    }

    fn step_viewer(&mut self, delta: isize) {
        if self.visible.is_empty() {
            return;
        }
        let len = self.visible.len() as isize;
        self.viewer_index = (self.viewer_index as isize + delta).rem_euclid(len) as usize;
        self.selection = Some(self.viewer_index);
        self.reset_zoom();
    }

    fn move_selection(&mut self, delta: isize) {
        if self.visible.is_empty() {
            return;
        }
        let len = self.visible.len() as isize;
        let current = self.selection.unwrap_or(0) as isize;
        self.selection = Some((current + delta).clamp(0, len - 1) as usize);
        self.scroll_to_selection = true;
    }

    pub(crate) fn show_toast(&mut self, message: String, error: bool) {
        self.toast = Some(Toast {
            message,
            error,
            expires: Instant::now() + Duration::from_secs(5),
        });
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let open_shortcut = ctx
            .input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O));
        if open_shortcut {
            self.pick_folder();
            return;
        }

        let search_shortcut = ctx
            .input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F));
        if search_shortcut {
            if self.viewer_open {
                self.close_viewer();
            }
            ctx.memory_mut(|memory| memory.request_focus(Id::unique(w::SEARCH_ID)));
            return;
        }

        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape {
            if self.crop_active {
                self.crop_active = false;
                self.crop_drag = None;
            } else if self.edit_panel_open {
                self.toggle_edit_panel();
            } else if self.viewer_open {
                self.close_viewer();
            } else {
                if !self.search.is_empty() {
                    self.search.clear();
                    self.sync_filters();
                }
                ctx.memory_mut(|memory| memory.surrender_focus(Id::unique(w::SEARCH_ID)));
            }
            return;
        }

        if ctx.egui_wants_keyboard_input() {
            return;
        }

        if self.viewer_open {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::E)) {
                self.toggle_edit_panel();
            }
            let zoom_in = ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::NONE, egui::Key::Plus)
                    || i.consume_key(egui::Modifiers::NONE, egui::Key::Equals)
            });
            let zoom_out = ctx
                .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Minus));
            let fit = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Num0));
            let actual =
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Num1));
            if zoom_in {
                let base = self.zoom.unwrap_or(self.viewer_fit);
                self.zoom = Some((base * 1.25).min(MAX_ZOOM));
            }
            if zoom_out {
                let base = self.zoom.unwrap_or(self.viewer_fit);
                let next = (base / 1.25).max(self.viewer_fit);
                self.zoom = if (next - self.viewer_fit).abs() < 0.001 {
                    None
                } else {
                    Some(next)
                };
            }
            if fit {
                self.zoom = None;
                self.pan = Vec2::ZERO;
            }
            if actual {
                self.zoom = Some(1.0);
                self.pan = Vec2::ZERO;
            }
            let left = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft));
            let right = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight));
            if left {
                self.step_viewer(-1);
            } else if right {
                self.step_viewer(1);
            }
            return;
        }

        let up = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
        let down = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
        let left = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft));
        let right = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight));
        let enter = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let home = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Home));
        let end = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::End));
        let columns = self.grid_columns.max(1) as isize;

        if home && !self.visible.is_empty() {
            self.selection = Some(0);
            self.scroll_to_selection = true;
        } else if end && !self.visible.is_empty() {
            self.selection = Some(self.visible.len() - 1);
            self.scroll_to_selection = true;
        }

        if right || down || left || up {
            if self.selection.is_none() {
                self.selection = Some(0);
                self.scroll_to_selection = true;
            } else if right {
                self.move_selection(1);
            } else if left {
                self.move_selection(-1);
            } else if down {
                self.move_selection(columns);
            } else if up {
                self.move_selection(-columns);
            }
        }

        if enter {
            if let Some(index) = self.selection {
                self.open_viewer(index);
            }
        }
    }

    fn toggle_edit_panel(&mut self) {
        if self.edit_panel_open {
            self.edit_panel_open = false;
            self.crop_active = false;
            self.crop_drag = None;
        } else {
            self.edit_panel_open = true;
            self.right_tab = RightTab::Edit;
        }
    }

    /// Open the right panel on `tab`; if it is already showing that tab, close
    /// the panel. Switching away from Edit leaves crop mode.
    fn toggle_right_tab(&mut self, tab: RightTab) {
        if self.edit_panel_open && self.right_tab == tab {
            self.edit_panel_open = false;
            self.crop_active = false;
            self.crop_drag = None;
        } else {
            self.edit_panel_open = true;
            self.right_tab = tab;
            if tab != RightTab::Edit {
                self.crop_active = false;
                self.crop_drag = None;
            }
        }
    }

    fn action_active(&self, id: ActionId) -> bool {
        match id {
            ActionId::Sliders => self.edit_panel_open && self.right_tab == RightTab::Edit,
            ActionId::Info => self.edit_panel_open && self.right_tab == RightTab::Info,
            ActionId::Crop => self.crop_active,
            _ => false,
        }
    }

    fn copy_current_path(&mut self, ctx: &egui::Context) {
        let Some(path) = self.current_viewer_path() else {
            return;
        };
        ctx.copy_text(path.display().to_string());
        self.show_toast("Image path copied".to_owned(), false);
    }

    /// Reveal the current image in the desktop file manager, selecting it when
    /// the default manager supports the freedesktop FileManager1 call.
    fn reveal_in_file_manager(&mut self) {
        let Some(path) = self.current_viewer_path() else {
            return;
        };
        self.reveal_rx = Some(crate::reveal::spawn(path));
        self.show_toast("Opening in file manager…".to_owned(), false);
    }

    fn poll_reveal(&mut self) {
        let Some(rx) = &self.reveal_rx else {
            return;
        };
        let Ok(result) = rx.try_recv() else {
            return;
        };
        self.reveal_rx = None;
        match result {
            Ok(()) => self.show_toast("Opened in file manager".to_owned(), false),
            Err(error) => {
                log::warn!("reveal failed: {error}");
                self.show_toast(format!("Could not open file manager: {error}"), true);
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        use w::Icon;
        let palette = self.palette;
        let metrics = palette.metrics;
        egui::Panel::top("top_bar")
            .exact_size(TOP_BAR_HEIGHT)
            .frame(
                Frame::new()
                    .fill(palette.header)
                    .inner_margin(Margin::symmetric(10, 8)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    if w::icon_button(ui, Icon::Menu, palette)
                        .on_hover_text("Toggle sidebar")
                        .clicked()
                    {
                        self.show_sidebar = !self.show_sidebar;
                    }

                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
                    ui.painter().rect_filled(
                        icon_rect,
                        CornerRadius::same(metrics.radius_sm),
                        palette.accent,
                    );
                    w::paint_icon(ui, Icon::Image, icon_rect.shrink(5.0), palette.on_accent);

                    ui.label(
                        RichText::new("Gallery")
                            .font(theme::bold_font(15.5))
                            .color(palette.text),
                    );

                    ui.add_space(metrics.space_xs);
                    for spec in crate::ui::actions::LEFT_ACTIONS {
                        self.render_action(ui, spec, palette);
                    }

                    // Image tools, only with an open image.
                    if self.viewer_open {
                        ui.add_space(metrics.space_sm);
                        ui.separator();
                        ui.add_space(metrics.space_xs);
                        for spec in crate::ui::actions::MIDDLE_ACTIONS {
                            self.render_action(ui, spec, palette);
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        for spec in crate::ui::actions::RIGHT_ACTIONS.iter().rev() {
                            self.render_action(ui, spec, palette);
                        }
                    });
                });
            });

        egui::Panel::top("header_separator")
            .exact_size(1.0)
            .frame(Frame::new().fill(palette.border))
            .show(ui, |_| {});
    }

    /// Render one top-bar action and dispatch it on click. The action lists
    /// live in `ui/actions.rs`, so moving or restyling a button is a data edit.
    fn render_action(&mut self, ui: &mut egui::Ui, spec: &ActionSpec, palette: Palette) {
        if spec.visible == Visibility::ViewerOpen && !self.viewer_open {
            return;
        }
        let active = self.action_active(spec.id);
        let is_toggle = matches!(
            spec.id,
            ActionId::Sliders | ActionId::Info | ActionId::Crop
        );
        let response = match spec.style {
            ButtonStyle::Primary => match spec.icon {
                Some(icon) => w::accent_icon_button(ui, icon, palette),
                None => w::accent_button(ui, spec.label, palette),
            },
            ButtonStyle::Ghost => match spec.icon {
                Some(icon) if is_toggle => w::toggle_icon_button(ui, icon, active, palette),
                Some(icon) => w::icon_button(ui, icon, palette),
                None => w::flat_button(ui, spec.label, palette),
            },
            ButtonStyle::Text => {
                let label = if spec.id == ActionId::Theme {
                    self.theme_mode.label()
                } else {
                    spec.label
                };
                w::flat_button(ui, label, palette)
            }
        };
        let response = response.on_hover_text(spec.label);
        if response.clicked() {
            let ctx = ui.ctx().clone();
            self.dispatch_action(spec.id, &ctx);
        }
    }

    fn dispatch_action(&mut self, id: ActionId, ctx: &egui::Context) {
        match id {
            ActionId::OpenFolder => self.pick_folder(),
            ActionId::RotateLeft => self.orientation_step(3),
            ActionId::RotateRight => self.orientation_step(1),
            ActionId::FlipV => self.flip(true),
            ActionId::FlipH => self.flip(false),
            ActionId::Crop => {
                if !self.edit_panel_open || self.right_tab != RightTab::Edit {
                    self.edit_panel_open = true;
                    self.right_tab = RightTab::Edit;
                    self.crop_drag = None;
                    self.crop_active = true;
                } else if self.crop_active {
                    self.crop_drag = None;
                    self.crop_active = false;
                } else {
                    self.crop_drag = None;
                    self.crop_active = true;
                }
            }
            ActionId::Sliders => self.toggle_right_tab(RightTab::Edit),
            ActionId::CopyPath => self.copy_current_path(ctx),
            ActionId::Info => self.toggle_right_tab(RightTab::Info),
            ActionId::Reveal => self.reveal_in_file_manager(),
            ActionId::Export => self.begin_export(),
            ActionId::Theme => {
                self.theme_mode = self.theme_mode.next();
                self.palette = Palette::from_mode(self.theme_mode);
                theme::apply(ctx, self.palette);
            }
            ActionId::Refresh => self.rescan(),
        }
    }

    fn orientation_step(&mut self, delta: u32) {
        let Some(path) = self.current_viewer_path() else {
            return;
        };
        self.crop_drag = None;
        let state = self.edits.entry(path).or_default();
        state.orientation_steps = (state.orientation_steps + delta) % 4;
        state.crop = None;
    }

    fn flip(&mut self, vertical: bool) {
        let Some(path) = self.current_viewer_path() else {
            return;
        };
        self.crop_drag = None;
        let state = self.edits.entry(path).or_default();
        if vertical {
            state.flip_v = !state.flip_v;
        } else {
            state.flip_h = !state.flip_h;
        }
        state.crop = None;
    }

    fn begin_export(&mut self) {
        if self.exporting {
            self.show_toast("An export is already running".to_owned(), false);
            return;
        }
        let Some(index) = self.visible.get(self.viewer_index).copied() else {
            return;
        };
        let path = self.all_items[index].path.clone();
        let edit = self.edits.get(&path).cloned().unwrap_or_default();
        self.export_rx = Some(crate::export::spawn(
            self.grade.device(),
            self.grade.queue(),
            path,
            edit,
        ));
        self.exporting = true;
        self.show_toast("Exporting full-resolution JPEG…".to_owned(), false);
    }

    fn poll_export(&mut self) {
        let Some(rx) = &self.export_rx else {
            return;
        };
        let Ok(outcome) = rx.try_recv() else {
            return;
        };
        self.export_rx = None;
        self.exporting = false;
        match outcome.result {
            Ok(paths) => {
                let name = paths
                    .image
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                log::info!(
                    "exported {name} + {} in {:.0} ms",
                    paths.sidecar.display(),
                    outcome.elapsed.as_secs_f64() * 1000.0
                );
                self.show_toast(format!("Exported {name} + sidecar"), false);
            }
            Err(error) => self.show_toast(format!("Export failed: {error}"), true),
        }
    }

    fn bottom_info_bar(&mut self, ui: &mut egui::Ui) {
        if !self.viewer_open {
            return;
        }
        let palette = self.palette;
        egui::Panel::bottom("info_bar")
            .exact_size(palette.metrics.bottom_bar_h)
            .frame(
                Frame::new()
                    .fill(palette.header)
                    .inner_margin(Margin::symmetric(12, 2)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    if let Some(item_index) = self.visible.get(self.viewer_index) {
                        let item = self.all_items[*item_index].clone();
                        ui.label(
                            RichText::new(&item.name)
                                .font(theme::bold_font(12.0))
                                .color(palette.text),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let dims = self
                                .viewer
                                .source_dimensions(&item.path)
                                .unwrap_or([0, 0]);
                            let kb = item.size as f64 / 1024.0;
                            let size_text = if kb > 1024.0 {
                                format!("{:.1} MB", kb / 1024.0)
                            } else {
                                format!("{:.0} KB", kb)
                            };
                            ui.label(
                                RichText::new(format!(
                                    "{} × {}  ·  {}",
                                    dims[0], dims[1], size_text
                                ))
                                .size(12.0)
                                .color(palette.dim),
                            );
                        });
                    }
                });
            });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette;
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(palette.metrics.sidebar_w)
            .size_range(190.0..=390.0)
            .frame(
                Frame::new()
                    .fill(palette.sidebar)
                    .inner_margin(Margin::symmetric(8, 10)),
            )
            .show(ui, |ui| {
                w::small_caps(ui, "Places", palette);
                ui.add_space(4.0);

                let places = self.places.clone();
                for place in &places {
                    let selected = self.current_dir.as_deref() == Some(place.path.as_path());
                    if sidebar_row(ui, place.name, selected, palette, true).clicked() {
                        self.navigate(place.path.clone());
                    }
                }

                ui.add_space(10.0);
                let line_rect = ui.available_rect_before_wrap();
                ui.painter().line_segment(
                    [
                        line_rect.left_top(),
                        egui::pos2(line_rect.right(), line_rect.top()),
                    ],
                    Stroke::new(1.0, palette.border),
                );
                ui.add_space(10.0);

                w::small_caps(ui, "Folders", palette);
                ui.add_space(2.0);

                let mut clicked = None;
                let selected = self.selected_folder.clone();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let expanded = &mut self.expanded;
                        tree_node_ui(
                            ui,
                            &mut self.root,
                            expanded,
                            selected.as_deref(),
                            palette,
                            0,
                            &mut clicked,
                        );
                    });
                if let Some(path) = clicked {
                    self.navigate(path);
                }
            });
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette;
        egui::CentralPanel::default()
            .frame(Frame::new().fill(palette.bg).inner_margin(Margin::same(0)))
            .show(ui, |ui| {
                if !self.viewer_open {
                    Frame::new()
                        .inner_margin(Margin::symmetric(18, 12))
                        .show(ui, |ui| {
                            self.content_header(ui);
                        });

                    let line_rect = ui.available_rect_before_wrap();
                    ui.painter().line_segment(
                        [
                            line_rect.left_top(),
                            egui::pos2(line_rect.right(), line_rect.top()),
                        ],
                        Stroke::new(1.0, palette.border),
                    );

                    ui.add_space(12.0);
                    self.grid_ui(ui);
                } else {
                    ui.add_space(4.0);
                    self.viewer_ui(ui);
                }
            });
    }

    fn content_header(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        // The one-row header needs room for the title plus the search box,
        // S/M/L, Recursive, Sort and the count. Below this it overlaps.
        let compact = ui.available_width() < 1000.0;

        if compact {
            ui.horizontal(|ui| {
                self.header_title(ui, palette);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    self.header_count(ui, palette);
                });
            });
            ui.add_space(8.0);

            let search_width = ui.available_width().clamp(160.0, 420.0);
            if w::search_field(ui, &mut self.search, search_width, palette) {
                self.sync_filters();
            }
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                self.header_sort(ui, palette);
                ui.add_space(6.0);
                self.header_size(ui);
                ui.add_space(6.0);
                self.header_recursive(ui, palette);
            });
        } else {
            ui.horizontal(|ui| {
                self.header_title(ui, palette);

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if w::search_field(ui, &mut self.search, 220.0, palette) {
                        self.sync_filters();
                    }

                    ui.add_space(6.0);
                    self.header_size(ui);

                    ui.add_space(6.0);
                    self.header_recursive(ui, palette);

                    ui.add_space(6.0);
                    self.header_sort(ui, palette);

                    ui.add_space(4.0);
                    self.header_count(ui, palette);
                });
            });
        }
    }

    fn header_title(&self, ui: &mut Ui, palette: Palette) {
        let title = self
            .current_dir
            .as_deref()
            .map(display_name)
            .unwrap_or_else(|| "No folder".to_owned());
        let subtitle = self
            .current_dir
            .as_deref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        // Bound the title block so a long path cannot squeeze the search box or
        // the S/M/L cluster; both lines ellipsize instead.
        ui.allocate_ui_with_layout(
            Vec2::new(340.0, 36.0),
            Layout::top_down(Align::LEFT),
            |ui| {
                ui.set_max_width(340.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add(
                    egui::Label::new(
                        RichText::new(title)
                            .font(theme::bold_font(17.0))
                            .color(palette.text),
                    )
                    .truncate(),
                );
                ui.add(
                    egui::Label::new(RichText::new(subtitle).size(11.5).color(palette.dim))
                        .truncate(),
                );
            },
        );
    }

    fn header_count(&self, ui: &mut Ui, palette: Palette) {
        if self.scanning {
            ui.spinner();
        } else {
            let count = self.visible.len();
            let label = if count == 1 {
                "1 image".to_owned()
            } else {
                format!("{count} images")
            };
            let response = ui.label(RichText::new(label).size(12.0).color(palette.dim));
            if let Some(elapsed) = self.scan_elapsed {
                response.on_hover_text(format!(
                    "Scanned in {:.1} ms",
                    elapsed.as_secs_f64() * 1000.0
                ));
            }
        }
    }

    fn header_sort(&mut self, ui: &mut Ui, palette: Palette) {
        if w::flat_button(ui, &format!("Sort: {}", self.sort.label()), palette).clicked() {
            self.sort = self.sort.next();
            self.sync_filters();
        }
        let arrow = if self.sort_desc { "↓" } else { "↑" };
        if w::symbol_button(ui, arrow, palette)
            .on_hover_text("Sort direction")
            .clicked()
        {
            self.sort_desc = !self.sort_desc;
            self.sync_filters();
        }
    }

    fn header_size(&mut self, ui: &mut Ui) {
        // A fixed-width dropdown keeps the header footprint stable and avoids
        // the S/M/L chip row colliding with the search field.
        let mut thumb_size = self.thumb_size;
        egui::ComboBox::from_id_salt("thumb-size")
            .selected_text(format!("Grid: {}", thumb_size.label()))
            .width(112.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut thumb_size, ThumbSize::Small, "Small");
                ui.selectable_value(&mut thumb_size, ThumbSize::Medium, "Medium");
                ui.selectable_value(&mut thumb_size, ThumbSize::Large, "Large");
            });
        if thumb_size != self.thumb_size {
            self.thumb_size = thumb_size;
        }
    }

    fn header_recursive(&mut self, ui: &mut Ui, palette: Palette) {
        // Keep the label/switch pair in a fixed cell: in the right-to-left
        // header the pair is otherwise laid out backwards, and the fixed width
        // stops the cell from resizing when the switch animates.
        let mut recursive = self.recursive;
        let changed = ui
            .allocate_ui_with_layout(
                Vec2::new(124.0, palette.metrics.control_h_md),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.label(RichText::new("Recursive").size(12.0).color(palette.dim));
                    ui.add_space(4.0);
                    w::switch(ui, &mut recursive, palette).changed()
                },
            )
            .inner;
        if changed {
            self.recursive = recursive;
            self.rescan();
        }
    }

    fn grid_ui(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let metrics = palette.metrics;
        let count = self.visible.len();

        if count == 0 {
            self.empty_state(ui);
            return;
        }

        let width = ui.available_width();
        let cell = self.thumb_size.px();
        let columns = (((width - metrics.grid_pad * 2.0 + metrics.grid_gap)
            / (cell + metrics.grid_gap))
            .floor() as usize)
            .max(1);
        self.grid_columns = columns;

        let cell_width = (width
            - metrics.grid_pad * 2.0
            - metrics.grid_gap * (columns.saturating_sub(1)) as f32)
            / columns as f32;
        let row_stride = cell_width + metrics.grid_gap;
        let rows = count.div_ceil(columns);

        let mut open_request = None;
        let mut select_request = None;
        let scroll_to = self.scroll_to_selection;
        let selection = self.selection;

        ui.add_space(2.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_stride, rows, |ui, row_range| {
                let base = ui.max_rect();
                let first_row = row_range.start;
                for row in row_range {
                    let top = base.top() + (row - first_row) as f32 * row_stride;
                    for column in 0..columns {
                        let index = row * columns + column;
                        if index >= count {
                            break;
                        }
                        let item_index = self.visible[index];
                        let item = &self.all_items[item_index];
                        let path = item.path.clone();
                        let name = item.name.clone();

                        let left = base.left()
                            + metrics.grid_pad
                            + column as f32 * (cell_width + metrics.grid_gap);
                        let cell_rect =
                            Rect::from_min_size(egui::pos2(left, top), Vec2::splat(cell_width));
                        let response = ui.interact(
                            cell_rect,
                            Id::unique(("thumb", index, &path)),
                            Sense::click(),
                        );

                        let thumbnail = self
                            .thumbs
                            .get(&path)
                            .map(|thumb| (thumb.texture.id(), thumb.size));
                        if thumbnail.is_none() {
                            self.thumbs.request(&path);
                        }

                        let selected = selection == Some(index);
                        draw_cell(
                            ui,
                            cell_rect,
                            &response,
                            palette,
                            thumbnail,
                            &name,
                            selected,
                        );

                        if selected && scroll_to {
                            ui.scroll_to_rect(cell_rect, Some(Align::Center));
                        }

                        if response.double_clicked() {
                            open_request = Some(index);
                        } else if response.clicked() {
                            select_request = Some(index);
                        }
                    }
                }
            });

        ui.add_space(10.0);
        self.scroll_to_selection = false;

        if let Some(index) = select_request {
            self.selection = Some(index);
        }
        if let Some(index) = open_request {
            self.open_viewer(index);
        }
    }

    fn empty_state(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let available = ui.available_rect_before_wrap();
        ui.allocate_ui_with_layout(
            available.size(),
            Layout::centered_and_justified(egui::Direction::TopDown),
            |ui| {
                ui.vertical_centered(|ui| {
                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::splat(54.0), Sense::hover());
                    w::paint_folder_icon(ui, icon_rect.shrink(6.0), palette.dim);

                    if self.current_dir.is_none() {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Open a folder to get started")
                                .size(15.0)
                                .color(palette.text),
                        );
                        ui.add_space(8.0);
                        if w::accent_button(ui, "Open Folder", palette).clicked() {
                            self.pick_folder();
                        }
                        return;
                    }

                    if self.scanning {
                        ui.add_space(10.0);
                        ui.spinner();
                        ui.add_space(4.0);
                        ui.label(RichText::new("Scanning folder…").color(palette.dim));
                        return;
                    }

                    let message = if !self.search.trim().is_empty() {
                        "No images match your search"
                    } else {
                        "No images in this folder"
                    };
                    ui.add_space(8.0);
                    ui.label(RichText::new(message).size(15.0).color(palette.text));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(if self.search.trim().is_empty() {
                            "Choose another folder from the sidebar, or turn on Recursive."
                        } else {
                            "Try a different search term."
                        })
                        .size(12.5)
                        .color(palette.dim),
                    );
                });
            },
        );
    }

    fn viewer_ui(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        if self.visible.is_empty() {
            self.close_viewer();
            return;
        }
        self.viewer_index = self.viewer_index.min(self.visible.len() - 1);
        let item_index = self.visible[self.viewer_index];
        let item = self.all_items[item_index].clone();

        Frame::new()
            .inner_margin(Margin::symmetric(18, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if w::flat_button(ui, "‹  Back", palette).clicked() {
                        self.close_viewer();
                    }
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(&item.name)
                            .font(theme::bold_font(15.0))
                            .color(palette.text),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} / {}",
                            self.viewer_index + 1,
                            self.visible.len()
                        ))
                        .size(12.0)
                        .color(palette.dim),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(dimensions) = self.viewer.source_dimensions(&item.path) {
                            ui.label(
                                RichText::new(format!("{} × {}", dimensions[0], dimensions[1]))
                                    .size(12.0)
                                    .color(palette.dim),
                            );
                        }
                        ui.label(
                            RichText::new(human_size(item.size))
                                .size(12.0)
                                .color(palette.dim),
                        );
                    });
                });
            });

        let stage = ui.available_rect_before_wrap();
        let available = stage.shrink(26.0);

        let dimensions = self
            .viewer
            .source_dimensions(&item.path)
            .or_else(|| self.thumbs.get(&item.path).map(|thumb| thumb.size));
        let thumbnail = self
            .thumbs
            .get(&item.path)
            .map(|thumb| (thumb.texture.id(), thumb.size));
        if thumbnail.is_none() {
            self.thumbs.request(&item.path);
        }

        // Effective edit: selecting the crop frame previews the full oriented
        // frame without crop or straighten so the overlay stays anchored.
        let state = self.edits.get(&item.path).cloned().unwrap_or_default();
        let preview = if self.crop_active {
            let mut preview = state.clone();
            preview.crop = None;
            preview.rotation = 0.0;
            preview
        } else {
            state
        };

        let display = dimensions.map(|source| preview.display_size(source));
        let drawn = display
            .map(|size| Vec2::new(size[0].max(1) as f32, size[1].max(1) as f32))
            .unwrap_or(Vec2::ONE);
        let fit = (available.width() / drawn.x)
            .min(available.height() / drawn.y)
            .min(1.0);
        self.viewer_fit = fit;
        let mut scale = self.zoom.unwrap_or(fit).clamp(fit, MAX_ZOOM);
        if self.crop_active {
            scale = fit;
            self.zoom = None;
            self.pan = Vec2::ZERO;
        }
        // Ask for a higher-resolution decode only in coarse zoom steps, so
        // wheel zooming does not thrash the loader.
        let target = viewer_target(
            stage,
            ui.ctx().pixels_per_point(),
            (scale / fit).max(1.0),
        );
        self.viewer.set_target(&item.path, target);

        let next_index = (self.viewer_index + 1) % self.visible.len();
        let next_path = self.all_items[self.visible[next_index]].path.clone();
        self.viewer.prefetch(&next_path, target);

        ui.painter().rect_filled(
            stage,
            CornerRadius::ZERO,
            if palette.dark {
                palette.bg
            } else {
                palette.card_hover
            },
        );

        let full_texture = self.viewer.texture_id(&item.path, target);
        // Route through the grade pass whenever the image has edits, whether or
        // not the panel is open, so adjustments stay visible after closing it.
        let mut texture_id = full_texture;
        if full_texture.is_some() {
            if let Some((base_id, base_view, base_size)) =
                self.viewer.base_texture(&item.path, target)
            {
                if let Some(source) = dimensions {
                    if !preview.is_identity() {
                        if let Some(graded) = self.grade.ensure(
                            &item.path,
                            &base_view,
                            base_id,
                            base_size,
                            source,
                            &preview,
                        ) {
                            texture_id = Some(graded);
                        }
                    }
                }
            }
        }

        // If the exact target is still decoding (zoom crossed a texture step),
        // keep drawing the last texture instead of flashing the thumbnail.
        if let Some(texture) = texture_id {
            self.viewer_display = Some((item.path.clone(), texture));
        } else if let Some((path, texture)) = &self.viewer_display {
            if path == &item.path {
                texture_id = Some(*texture);
            }
        }

        if let Some(crate_texture_id) = texture_id {
            let size = drawn * scale;
            // Keep the image from being dragged completely out of view.
            let max_x = ((size.x - available.width()) * 0.5).max(0.0);
            let max_y = ((size.y - available.height()) * 0.5).max(0.0);
            self.pan.x = self.pan.x.clamp(-max_x, max_x);
            self.pan.y = self.pan.y.clamp(-max_y, max_y);
            let rect = Rect::from_center_size(available.center() + self.pan, size);
            if size.x < available.width() - 1.0 || size.y < available.height() - 1.0 {
                ui.painter()
                    .rect_filled(rect.expand(2.0), CornerRadius::same(4), palette.shadow);
            }
            draw_texture(ui, crate_texture_id, rect, Color32::WHITE);
            if self.edit_panel_open && self.right_tab == RightTab::Edit && self.crop_active {
                if let Some(source) = dimensions {
                    let source = [source[0] as f32, source[1] as f32];
                    crate::ui::edit_panel::crop_overlay(self, ui, rect, source);
                }
            } else {
                self.viewer_canvas_input(ui, available, fit, scale);
                self.zoom_pill(ui, available, palette, fit, scale);
            }
        } else if let Some(error) = &self.viewer.error {
            ui.allocate_ui_with_layout(
                stage.size(),
                Layout::centered_and_justified(egui::Direction::TopDown),
                |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("Could not open image")
                                .size(16.0)
                                .color(palette.text),
                        );
                        ui.add_space(4.0);
                        ui.label(RichText::new(error).size(12.0).color(palette.dim));
                    });
                },
            );
        } else {
            if let Some((texture_id, size)) = thumbnail {
                let texture_size = Vec2::new(size[0].max(1) as f32, size[1].max(1) as f32);
                let scale = (available.width() / texture_size.x)
                    .min(available.height() / texture_size.y);
                let scaled = texture_size * scale.max(0.01);
                let rect = Rect::from_center_size(available.center(), scaled);
                draw_texture(
                    ui,
                    texture_id,
                    rect,
                    Color32::from_white_alpha(225),
                );
            }

            let spinner = egui::pos2(stage.right() - 28.0, stage.top() + 28.0);
            ui.put(
                Rect::from_center_size(spinner, Vec2::splat(24.0)),
                egui::Spinner::new().size(20.0),
            );
        }
    }

    /// Wheel zoom at the cursor, drag pan, and double-click Fit/100%. Only
    /// active when the crop overlay is not owning the pointer.
    fn viewer_canvas_input(&mut self, ui: &mut Ui, available: Rect, fit: f32, scale: f32) {
        let response = ui.interact(available, Id::unique("viewer-canvas"), Sense::click_and_drag());
        if response.double_clicked() {
            if self.zoom.is_none() {
                self.zoom = Some(1.0f32.max(fit));
            } else {
                self.zoom = None;
            }
            self.pan = Vec2::ZERO;
        }

        let can_pan = scale > fit + 0.001;
        if can_pan && (response.hovered() || response.dragged()) {
            ui.ctx().set_cursor_icon(if response.dragged() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
        }
        if can_pan && response.dragged() {
            self.pan += response.drag_delta();
        }

        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll.abs() > 0.01 {
                let pointer = ui
                    .input(|input| input.pointer.hover_pos())
                    .unwrap_or(available.center());
                let factor = (scroll * 0.002).exp();
                let next = (scale * factor).clamp(fit, MAX_ZOOM);
                if (next - scale).abs() > 0.001 {
                    // Keep the image point under the pointer fixed.
                    let cursor = pointer - available.center();
                    self.pan = cursor - (cursor - self.pan) * (next / scale);
                    self.zoom = if (next - fit).abs() < 0.001 {
                        None
                    } else {
                        Some(next)
                    };
                }
            }
        }
    }

    /// Floating zoom controls in the bottom-right of the preview box.
    fn zoom_pill(&mut self, ui: &mut Ui, available: Rect, palette: Palette, fit: f32, scale: f32) {
        let width = 172.0;
        let height = 44.0;
        let origin = egui::pos2(
            available.right() - 16.0 - width,
            available.bottom() - 16.0 - height,
        );
        egui::Area::new(Id::unique("viewer-zoom-controls"))
            .order(egui::Order::Foreground)
            .fixed_pos(origin)
            .show(ui.ctx(), |ui| {
                Frame::new()
                    .fill(palette.card)
                    .stroke(Stroke::new(1.0, palette.input))
                    .corner_radius(CornerRadius::same(palette.metrics.radius_lg))
                    .inner_margin(Margin::same(3))
                    .show(ui, |ui| {
                        ui.horizontal_centered(|ui| {
                            if w::icon_button(ui, w::Icon::ZoomOut, palette)
                                .on_hover_text("Zoom out")
                                .clicked()
                            {
                                let next = (scale / 1.25).max(fit);
                                self.zoom = if (next - fit).abs() < 0.001 {
                                    None
                                } else {
                                    Some(next)
                                };
                            }
                            ui.label(
                                RichText::new(format!("{:.0}%", scale * 100.0))
                                    .size(11.5)
                                    .color(palette.dim),
                            );
                            if w::icon_button(ui, w::Icon::ZoomIn, palette)
                                .on_hover_text("Zoom in")
                                .clicked()
                            {
                                self.zoom = Some((scale * 1.25).min(MAX_ZOOM));
                            }
                            if w::icon_button(ui, w::Icon::Maximize, palette)
                                .on_hover_text("Fit to window")
                                .clicked()
                            {
                                self.zoom = None;
                                self.pan = Vec2::ZERO;
                            }
                        });
                    });
            });
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_owned())
                .collect()
        });
        for path in dropped {
            if path.is_dir() {
                self.navigate(path);
                break;
            }
            if is_image(&path) {
                if let Some(parent) = path.parent() {
                    let parent = parent.to_path_buf();
                    self.navigate(parent);
                    self.pending_open = Some(path);
                    break;
                }
            }
        }
    }

    fn toast(&mut self, ctx: &egui::Context) {
        let palette = self.palette;
        let Some(toast) = &self.toast else {
            return;
        };
        if Instant::now() >= toast.expires {
            self.toast = None;
            return;
        }
        let fill = if toast.error {
            palette.error
        } else {
            palette.sidebar
        };
        egui::Area::new(Id::unique("toast"))
            .anchor(Align2::LEFT_BOTTOM, Vec2::new(18.0, -18.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(fill)
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(14, 10))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 4],
                        blur: 18,
                        spread: 0,
                        color: palette.shadow,
                    })
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(&toast.message)
                                .size(12.5)
                                .color(if toast.error {
                                    Color32::WHITE
                                } else {
                                    palette.text
                                }),
                        );
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(200));
    }

}

impl eframe::App for GalleryApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_dropped_files(&ctx);
        self.thumbs.poll(&ctx);
        self.viewer.poll();
        self.poll_scan();
        self.poll_export();
        self.poll_reveal();
        self.handle_keys(&ctx);

        self.top_bar(ui);
        if self.show_sidebar {
            self.sidebar(ui);
        }
        self.bottom_info_bar(ui);
        if self.viewer_open && self.edit_panel_open {
            crate::ui::edit_panel::show(self, ui);
        }
        self.central(ui);
        self.sync_filters();
        self.toast(&ctx);

        if self.scanning
            || self.viewer.loading
            || self.thumbs.busy()
            || self.exporting
            || self.reveal_rx.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(60));
        }
    }

}

fn sidebar_row(
    ui: &mut Ui,
    label: &str,
    selected: bool,
    palette: Palette,
    icon: bool,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if selected {
            palette.accent
        } else if response.hovered() {
            palette.card_hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(7), fill);
        let content = if selected {
            palette.on_accent
        } else {
            palette.text
        };
        let mut text_x = rect.left() + 8.0;
        if icon {
            let icon_rect = Rect::from_min_size(
                egui::pos2(rect.left() + 8.0, rect.center().y - 7.0),
                Vec2::new(16.0, 14.0),
            );
            w::paint_folder_icon(ui, icon_rect, content.gamma_multiply(0.9));
            text_x = icon_rect.right() + 8.0;
        }
        ui.painter()
            .with_clip_rect(rect.shrink2(Vec2::new(4.0, 0.0)))
            .text(
                egui::pos2(text_x, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(13.5),
                content,
            );
    }
    response
}

#[allow(clippy::too_many_arguments)]
fn tree_node_ui(
    ui: &mut Ui,
    node: &mut FolderNode,
    expanded: &mut HashSet<PathBuf>,
    selected: Option<&Path>,
    palette: Palette,
    depth: usize,
    clicked: &mut Option<PathBuf>,
) {
    let is_expanded = expanded.contains(&node.path);
    let is_selected = selected == Some(node.path.as_path());
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click());
    let indent = depth as f32 * 15.0;
    let chevron_rect = Rect::from_min_size(
        egui::pos2(rect.left() + indent + 2.0, rect.top()),
        Vec2::new(18.0, rect.height()),
    );
    let chevron = ui.interact(
        chevron_rect,
        Id::unique(("tree-chevron", &node.path)),
        Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        let fill = if is_selected {
            palette.accent
        } else if response.hovered() || chevron.hovered() {
            palette.card_hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(7), fill);
        let content = if is_selected {
            palette.on_accent
        } else {
            palette.text
        };

        if node.has_subdirs {
            paint_chevron(ui, chevron_rect.center(), is_expanded, content.gamma_multiply(0.8));
        }

        let icon_rect = Rect::from_min_size(
            egui::pos2(chevron_rect.right() + 2.0, rect.center().y - 7.0),
            Vec2::new(16.0, 14.0),
        );
        w::paint_folder_icon(ui, icon_rect, content.gamma_multiply(0.9));

        ui.painter()
            .with_clip_rect(rect.shrink2(Vec2::new(4.0, 0.0)))
            .text(
                egui::pos2(icon_rect.right() + 7.0, rect.center().y),
                Align2::LEFT_CENTER,
                &node.name,
                FontId::proportional(13.0),
                content,
            );
    }

    if chevron.clicked() && node.has_subdirs {
        if is_expanded {
            expanded.remove(&node.path);
        } else {
            node.ensure_loaded();
            expanded.insert(node.path.clone());
        }
    } else if response.clicked() {
        *clicked = Some(node.path.clone());
    }

    if expanded.contains(&node.path) {
        node.ensure_loaded();
        if let Some(children) = node.children.as_mut() {
            for child in children {
                tree_node_ui(
                    ui, child, expanded, selected, palette, depth + 1, clicked,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_cell(
    ui: &Ui,
    rect: Rect,
    response: &egui::Response,
    palette: Palette,
    thumbnail: Option<(egui::TextureId, [u32; 2])>,
    name: &str,
    selected: bool,
) {
    let painter = ui.painter();
    let hovered = response.hovered();
    let background = if selected {
        palette.accent.gamma_multiply(0.22)
    } else if hovered {
        palette.card_hover
    } else {
        palette.card
    };
    painter.rect_filled(rect, CornerRadius::same(12), background);

    if let Some((texture_id, size)) = thumbnail {
        let inner = rect.shrink(3.0);
        let (texture_w, texture_h) = (size[0].max(1) as f32, size[1].max(1) as f32);
        let texture_aspect = texture_w / texture_h;
        let rect_aspect = inner.width() / inner.height();
        let mut uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        if texture_aspect > rect_aspect {
            let fraction = rect_aspect / texture_aspect;
            let inset = (1.0 - fraction) * 0.5;
            uv.min.x = inset;
            uv.max.x = 1.0 - inset;
        } else {
            let fraction = texture_aspect / rect_aspect;
            let inset = (1.0 - fraction) * 0.5;
            uv.min.y = inset;
            uv.max.y = 1.0 - inset;
        }
        egui::Image::new((texture_id, Vec2::new(texture_w, texture_h)))
            .uv(uv)
            .corner_radius(CornerRadius::same(9))
            .paint_at(ui, inner);
    } else {
        let icon_rect = Rect::from_center_size(rect.center(), Vec2::new(26.0, 22.0));
        w::paint_image_icon(ui, icon_rect, palette.dim.gamma_multiply(0.55));
    }

    if hovered {
        let bar = Rect::from_min_max(
            egui::pos2(rect.left() + 4.0, rect.bottom() - 27.0),
            egui::pos2(rect.right() - 4.0, rect.bottom() - 4.0),
        );
        painter.rect_filled(bar, CornerRadius::same(7), Color32::from_black_alpha(155));
        let max_chars = ((bar.width() - 16.0) / 6.6).max(4.0) as usize;
        let label = truncate_name(name, max_chars);
        painter.text(
            egui::pos2(bar.left() + 8.0, bar.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(11.5),
            Color32::WHITE,
        );
    }

    if selected {
        painter.rect_stroke(
            rect.shrink(1.0),
            CornerRadius::same(11),
            Stroke::new(2.5, palette.accent),
            StrokeKind::Inside,
        );

        let center = egui::pos2(rect.right() - 17.0, rect.top() + 17.0);
        painter.circle_filled(center, 9.5, palette.accent);
        painter.line_segment(
            [
                egui::pos2(center.x - 4.0, center.y),
                egui::pos2(center.x - 1.0, center.y + 3.0),
            ],
            Stroke::new(1.7, palette.on_accent),
        );
        painter.line_segment(
            [
                egui::pos2(center.x - 1.0, center.y + 3.0),
                egui::pos2(center.x + 4.0, center.y - 3.0),
            ],
            Stroke::new(1.7, palette.on_accent),
        );
    }
}

fn draw_texture(ui: &Ui, texture_id: egui::TextureId, rect: Rect, tint: Color32) {
    ui.painter().image(
        texture_id,
        rect,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        tint,
    );
}

fn viewer_target(stage: Rect, pixels_per_point: f32, zoom_factor: f32) -> [u32; 2] {
    // Decode extra resolution only in coarse steps so wheel zooming does not
    // queue a new decode for every notch.
    let factor = if zoom_factor >= 6.0 {
        8.0
    } else if zoom_factor >= 3.0 {
        4.0
    } else if zoom_factor >= 1.5 {
        2.0
    } else {
        1.0
    };
    let scale = pixels_per_point.max(1.0) * 1.15 * factor;
    let quantize = |value: f32| -> u32 {
        let value = (value.max(1.0) * scale).ceil() as u32;
        (value.div_ceil(128) * 128).clamp(512, 4096)
    };
    [quantize(stage.width()), quantize(stage.height())]
}

fn truncate_name(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        return name.to_owned();
    }
    let mut result: String = name.chars().take(max_chars.saturating_sub(1)).collect();
    result.push('…');
    result
}

fn paint_chevron(ui: &Ui, center: egui::Pos2, expanded: bool, color: Color32) {
    let painter = ui.painter();
    let stroke = Stroke::new(1.5, color);
    if expanded {
        painter.line_segment(
            [
                egui::pos2(center.x - 4.0, center.y - 2.0),
                egui::pos2(center.x, center.y + 2.0),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(center.x, center.y + 2.0),
                egui::pos2(center.x + 4.0, center.y - 2.0),
            ],
            stroke,
        );
    } else {
        painter.line_segment(
            [
                egui::pos2(center.x - 2.0, center.y - 4.0),
                egui::pos2(center.x + 2.0, center.y),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(center.x + 2.0, center.y),
                egui::pos2(center.x - 2.0, center.y + 4.0),
            ],
            stroke,
        );
    }
}

fn collect_places() -> Vec<Place> {
    let mut places = Vec::new();
    let mut seen = HashSet::new();

    let mut push = |name: &'static str, path: Option<PathBuf>| {
        if let Some(path) = path {
            if path.is_dir() && seen.insert(path.clone()) {
                places.push(Place { name, path });
            }
        }
    };

    push("Home", dirs::home_dir());
    push("Pictures", dirs::picture_dir());
    push("Downloads", dirs::download_dir());
    push("Desktop", dirs::desktop_dir());

    places
}
