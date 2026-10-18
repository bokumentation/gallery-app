use crate::grade::{gpu::GradePass, EditState};
use crate::model::{is_image, ImageItem, SortKey, ThumbSize};
use crate::scan::{spawn_scan, ScanOutcome};
use crate::theme::{self, Palette, ThemeMode};
use crate::thumbs::{human_size, ThumbnailStore};
use crate::viewer::ViewerService;
use crate::ui::widgets as w;
use crossbeam_channel::Receiver;
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, Rect, RichText, Sense,
    Stroke, StrokeKind, Ui, Vec2,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const GRID_GAP: f32 = 12.0;
const GRID_PAD: f32 = 18.0;

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
    scan_generation: u64,
    scan_rx: Option<Receiver<ScanOutcome>>,
    scanning: bool,
    scan_elapsed: Option<Duration>,
    thumbs: ThumbnailStore,
    pub(crate) viewer: ViewerService,
    pub(crate) edit_panel_open: bool,
    pub(crate) edits: HashMap<PathBuf, EditState>,
    pub(crate) grade: GradePass,
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
            edits: HashMap::new(),
            grade: GradePass::new(
                cc.wgpu_render_state
                    .as_ref()
                    .expect("wgpu render state is available with the wgpu renderer"),
            ),
            toast: None,
            pending_open: None,
            last_search: String::new(),
            last_sort: SortKey::Name,
            last_sort_desc: false,
        };

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

    fn pick_folder(&mut self) {
        log::info!("folder picker: opening");
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
    }

    fn close_viewer(&mut self) {
        self.viewer_open = false;
        self.viewer.deactivate();
        self.grade.release();
    }

    /// Path of the image currently shown in the viewer.
    pub(crate) fn current_viewer_path(&self) -> Option<PathBuf> {
        self.visible
            .get(self.viewer_index)
            .and_then(|index| self.all_items.get(*index))
            .map(|item| item.path.clone())
    }

    fn step_viewer(&mut self, delta: isize) {
        if self.visible.is_empty() {
            return;
        }
        let len = self.visible.len() as isize;
        self.viewer_index = (self.viewer_index as isize + delta).rem_euclid(len) as usize;
        self.selection = Some(self.viewer_index);
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

    fn show_toast(&mut self, message: String, error: bool) {
        self.toast = Some(Toast {
            message,
            error,
            expires: Instant::now() + Duration::from_secs(5),
        });
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        log::debug!(
            "handle_keys focus={} escape={}",
            ctx.memory(|m| m.focused().is_some()),
            ctx.input(|i| i.key_pressed(egui::Key::Escape))
        );
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
            ctx.memory_mut(|memory| memory.request_focus(Id::new(w::SEARCH_ID)));
            return;
        }

        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape {
            if self.edit_panel_open {
                self.toggle_edit_panel();
            } else if self.viewer_open {
                self.close_viewer();
            } else {
                if !self.search.is_empty() {
                    self.search.clear();
                    self.sync_filters();
                }
                ctx.memory_mut(|memory| memory.surrender_focus(Id::new(w::SEARCH_ID)));
            }
            return;
        }

        if ctx.wants_keyboard_input() {
            return;
        }

        if self.viewer_open {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::E)) {
                self.toggle_edit_panel();
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
        self.edit_panel_open = !self.edit_panel_open;
        if !self.edit_panel_open {
            self.grade.release();
        }
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        let palette = self.palette;
        egui::TopBottomPanel::top("top_bar")
            .exact_height(52.0)
            .frame(
                Frame::new()
                    .fill(palette.header)
                    .inner_margin(Margin::symmetric(10, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if w::symbol_button(ui, "☰", palette)
                        .on_hover_text("Toggle sidebar")
                        .clicked()
                    {
                        self.show_sidebar = !self.show_sidebar;
                    }

                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
                    ui.painter()
                        .rect_filled(icon_rect, CornerRadius::same(7), palette.accent);
                    w::paint_image_icon(ui, icon_rect.shrink(6.0), palette.on_accent);

                    ui.label(
                        RichText::new("Gallery")
                            .font(theme::bold_font(15.5))
                            .color(palette.text),
                    );

                    ui.add_space(10.0);
                    if w::flat_button(ui, "Open Folder", palette).clicked() {
                        self.pick_folder();
                    }
                    if self.viewer_open
                        && w::flat_button(ui, "Adjust", palette)
                            .on_hover_text("Crop and color grade (E)")
                            .clicked()
                    {
                        self.toggle_edit_panel();
                    }
                    if w::symbol_button(ui, "⟳", palette)
                        .on_hover_text("Refresh")
                        .clicked()
                    {
                        self.rescan();
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mode = self.theme_mode;
                        if w::flat_button(ui, mode.label(), palette)
                            .on_hover_text("Theme: Auto, Light, Dark")
                            .clicked()
                        {
                            self.theme_mode = self.theme_mode.next();
                            self.palette = Palette::from_mode(self.theme_mode);
                            theme::apply(ctx, self.palette);
                        }
                        ui.add_space(4.0);
                    });
                });
            });

        egui::TopBottomPanel::top("header_separator")
            .exact_height(1.0)
            .frame(Frame::new().fill(palette.border))
            .show(ctx, |_| {});
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        let palette = self.palette;
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(252.0)
            .width_range(190.0..=390.0)
            .frame(
                Frame::new()
                    .fill(palette.sidebar)
                    .inner_margin(Margin::symmetric(8, 10)),
            )
            .show(ctx, |ui| {
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

    fn central(&mut self, ctx: &egui::Context) {
        let palette = self.palette;
        egui::CentralPanel::default()
            .frame(Frame::new().fill(palette.bg).inner_margin(Margin::same(0)))
            .show(ctx, |ui| {
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
        let compact = ui.available_width() < 820.0;

        if compact {
            ui.horizontal(|ui| {
                self.header_title(ui, palette);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    self.header_count(ui, palette);
                });
            });
            ui.add_space(8.0);

            let search_width = (ui.available_width() - 86.0).clamp(120.0, 320.0);
            if w::search_field(ui, &mut self.search, search_width, palette) {
                self.sync_filters();
            }
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                self.header_sort(ui, palette);
                ui.add_space(6.0);
                self.header_size(ui, palette);
                ui.add_space(6.0);
                self.header_recursive(ui, palette);
            });
        } else {
            ui.horizontal(|ui| {
                self.header_title(ui, palette);

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if w::search_field(ui, &mut self.search, 170.0, palette) {
                        self.sync_filters();
                    }

                    ui.add_space(6.0);
                    self.header_size(ui, palette);

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
        ui.vertical(|ui| {
            ui.label(
                RichText::new(title)
                    .font(theme::bold_font(17.0))
                    .color(palette.text),
            );
            let subtitle = self
                .current_dir
                .as_deref()
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            ui.label(RichText::new(subtitle).size(11.5).color(palette.dim));
        });
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

    fn header_size(&mut self, ui: &mut Ui, palette: Palette) {
        let mut thumb_size = self.thumb_size;
        ui.allocate_ui_with_layout(
            Vec2::new(104.0, 30.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                if w::segmented(
                    ui,
                    &[
                        (ThumbSize::Small, "S"),
                        (ThumbSize::Medium, "M"),
                        (ThumbSize::Large, "L"),
                    ],
                    &mut thumb_size,
                    palette,
                ) {
                    self.thumb_size = thumb_size;
                }
            },
        );
    }

    fn header_recursive(&mut self, ui: &mut Ui, palette: Palette) {
        ui.allocate_ui_with_layout(
            Vec2::new(114.0, 30.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.label(RichText::new("Recursive").size(12.0).color(palette.dim));
                let mut recursive = self.recursive;
                if w::switch(ui, &mut recursive, palette).changed() {
                    self.recursive = recursive;
                    self.rescan();
                }
            },
        );
    }

    fn grid_ui(&mut self, ui: &mut Ui) {
        let palette = self.palette;
        let count = self.visible.len();

        if count == 0 {
            self.empty_state(ui);
            return;
        }

        let width = ui.available_width();
        let cell = self.thumb_size.px();
        let columns = (((width - GRID_PAD * 2.0 + GRID_GAP) / (cell + GRID_GAP)).floor() as usize)
            .max(1);
        self.grid_columns = columns;

        let cell_width =
            (width - GRID_PAD * 2.0 - GRID_GAP * (columns.saturating_sub(1)) as f32)
                / columns as f32;
        let row_stride = cell_width + GRID_GAP;
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

                        let left = base.left() + GRID_PAD + column as f32 * (cell_width + GRID_GAP);
                        let cell_rect =
                            Rect::from_min_size(egui::pos2(left, top), Vec2::splat(cell_width));
                        let response = ui.interact(
                            cell_rect,
                            Id::new(("thumb", index, &path)),
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
        let target = viewer_target(stage, ui.ctx().pixels_per_point());
        self.viewer.set_target(&item.path, target);

        let next_index = (self.viewer_index + 1) % self.visible.len();
        let next_path = self.all_items[self.visible[next_index]].path.clone();
        self.viewer.prefetch(&next_path, target);

        ui.painter().rect_filled(
            stage,
            CornerRadius::ZERO,
            if palette.dark {
                Color32::from_rgb(23, 23, 23)
            } else {
                Color32::from_rgb(246, 246, 246)
            },
        );

        let available = stage.shrink(26.0);
        let full_texture = self.viewer.texture_id(&item.path, target);
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

        // Route through the grade pass when the panel is open.
        let mut texture_id = full_texture;
        let mut drawn_size = dimensions.map(|d| Vec2::new(d[0] as f32, d[1] as f32));
        if self.edit_panel_open && full_texture.is_some() && dimensions.is_some() {
            let base = self.viewer.base_texture(&item.path, target);
            if let Some((base_id, base_view, base_size)) = base {
                let state = self.edits.get(&item.path).cloned().unwrap_or_default();
                if !state.is_identity() {
                    let graded = self.grade.ensure(
                        &item.path,
                        &base_view,
                        base_id,
                        base_size,
                        dimensions.unwrap(),
                        &state,
                    );
                    if let Some(graded) = graded {
                        texture_id = Some(graded);
                        let display = state.display_size(dimensions.unwrap_or(base_size));
                        drawn_size = Some(Vec2::new(display[0] as f32, display[1] as f32));
                    }
                }
            }
        }

        if let Some(crate_texture_id) = texture_id {
            if let Some(texture_size) = drawn_size {
                let scale = (available.width() / texture_size.x)
                    .min(available.height() / texture_size.y)
                    .min(1.0);
                let size = texture_size * scale.max(0.01);
                let rect = Rect::from_center_size(available.center(), size);
                if scale < 1.0 {
                    ui.painter()
                        .rect_filled(rect.expand(2.0), CornerRadius::same(4), palette.shadow);
                }
                draw_texture(ui, crate_texture_id, rect, Color32::WHITE);
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

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
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
        egui::Area::new(Id::new("toast"))
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_dropped_files(ctx);
        self.thumbs.poll(ctx);
        self.viewer.poll();
        self.poll_scan();
        self.handle_keys(ctx);

        self.top_bar(ctx);
        if self.show_sidebar {
            self.sidebar(ctx);
        }
        if self.viewer_open && self.edit_panel_open {
            crate::ui::edit_panel::show(self, ctx);
        }
        self.central(ctx);
        self.sync_filters();
        self.toast(ctx);

        if self.scanning || self.viewer.loading || self.thumbs.busy() {
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
        Id::new(("tree-chevron", &node.path)),
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

fn viewer_target(stage: Rect, pixels_per_point: f32) -> [u32; 2] {
    let scale = pixels_per_point.max(1.0) * 1.15;
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
