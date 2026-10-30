//! Preset browser: every kit and every pattern in one searchable list, with star ratings and tags
//! (favorites.rs). Opens from the display or the BROWSER button; clicking an item loads it. SAVE
//! saves the current kit, the shown pattern or the whole session into the library (save.rs); IMPORT
//! copies an LXR-02 download (zip or folder, e.g. Erica Synths' factory content) into it.

use super::*;
use crate::favorites::{self, Favorites};
use crate::seq;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Kits,
    Patterns,
}

pub(super) struct BrowserState {
    tab: Tab,
    query: String,
    favorites_only: bool,
    sort_by_stars: bool,
    /// bank (kits) or project (patterns) filter
    group: Option<String>,
    tag: Option<String>,
    selected: Option<String>,
    new_tag: String,
    favorites: Favorites,
    /// the right column shows the save form instead of the selected item
    saving: bool,
    /// ... or the import form
    importing: bool,
    /// a file chooser is open: what it picks (Some(None): cancelled) comes back here
    picking: Option<std::sync::Arc<std::sync::Mutex<Option<Option<std::path::PathBuf>>>>>,
    save_name: String,
    save_focus: bool,
    /// result of the last save and until when to show it
    message: Option<(String, f64)>,
}

impl BrowserState {
    pub fn new(patterns: bool) -> Self {
        Self {
            tab: if patterns { Tab::Patterns } else { Tab::Kits },
            query: String::new(),
            favorites_only: false,
            sort_by_stars: false,
            group: None,
            tag: None,
            selected: None,
            new_tag: String::new(),
            favorites: Favorites::load(),
            saving: false,
            importing: false,
            picking: None,
            save_name: String::new(),
            save_focus: false,
            message: None,
        }
    }
}

/// One row of the list.
struct Item {
    key: String,
    name: String,
    group: String,
    detail: String,
    target: Target,
}

enum Target {
    Kit(usize),
    Pattern { project: usize, nr: usize },
}

fn items(st: &State, tab: Tab) -> Vec<Item> {
    match tab {
        Tab::Kits => st
            .kits
            .iter()
            .enumerate()
            .map(|(i, k)| Item {
                key: favorites::kit_key(&k.name, &k.data),
                name: k.name.clone(),
                group: k.bank.clone(),
                detail: String::new(),
                target: Target::Kit(i),
            })
            .collect(),
        Tab::Patterns => st
            .projects
            .iter()
            .enumerate()
            .flat_map(|(pi, project)| {
                project.patterns.iter().enumerate().filter_map(move |(nr, p)| {
                    let p = p.as_ref().filter(|p| !seq::is_empty(p))?;
                    let kit = project.kit(seq::kit_number(p)).map(|k| k.0.clone()).unwrap_or_default();
                    Some(Item {
                        key: favorites::pattern_key(&project.name, nr, p),
                        name: format!("P{:02}", nr + 1),
                        group: project.name.clone(),
                        detail: kit,
                        target: Target::Pattern { project: pi, nr },
                    })
                })
            })
            .collect(),
    }
}

/// A five pointed star.
fn star(painter: &egui::Painter, center: Pos2, r: f32, filled: bool) {
    let points: Vec<Pos2> = (0..10)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            let radius = if i % 2 == 0 { r } else { r * 0.45 };
            center + Vec2::angled(a) * radius
        })
        .collect();
    if filled {
        // a star isn't convex: fill it as five triangles around the middle
        for i in 0..10 {
            painter.add(egui::Shape::convex_polygon(
                vec![center, points[i], points[(i + 1) % 10]],
                Color32::from_rgb(255, 196, 60),
                Stroke::NONE,
            ));
        }
    } else {
        painter.add(egui::Shape::closed_line(points, Stroke::new(1.0_f32, LABEL_DIM)));
    }
}

/// Five clickable stars; returns the new rating if one was clicked (the current one clears it).
fn stars(ui: &mut Ui, origin: Pos2, size: f32, rating: u8) -> Option<u8> {
    let mut clicked = None;
    for i in 1..=5u8 {
        let center = origin + Vec2::new((i as f32 - 0.5) * size * 1.1, 0.0);
        let rect = Rect::from_center_size(center, Vec2::splat(size));
        let resp = ui.allocate_rect(rect, Sense::click());
        star(ui.painter(), center, size * 0.45, i <= rating);
        if resp.clicked() {
            clicked = Some(if rating == i { 0 } else { i });
        }
    }
    clicked
}

fn button(ui: &mut Ui, rect: Rect, text: &str, lit: bool) -> Response {
    let resp = ui.allocate_rect(rect, Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, if lit { LED_RED } else if resp.hovered() { BUTTON } else { BUTTON_DIM });
    painter.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(12.0), if lit { PANEL } else { LABEL });
    resp
}

fn load(st: &mut State, setter: &ParamSetter, target: &Target) {
    match *target {
        Target::Kit(i) => {
            st.kit_index = i;
            st.load_to_morph = false;
            st.load_selected(setter);
        }
        Target::Pattern { project, nr } => {
            st.proj_sel = project;
            st.pat_sel = nr;
            sequencer::apply(st);
        }
    }
}

/// What the save form's name starts as: the kit's name, or the session's project.
fn default_save_name(st: &State, tab: Tab) -> String {
    match tab {
        Tab::Kits => st.params.kit_name.lock().map(|n| n.clone()).unwrap_or_default(),
        Tab::Patterns => st.params.seq.project_name().unwrap_or_else(|| "MY PATTERNS".to_string()),
    }
}

/// The right column while SAVE is lit: a name and what to save.
fn save_form(ui: &mut Ui, st: &mut State, b: &mut BrowserState, right: Rect) {
    let painter = ui.painter().clone();
    let now = ui.input(|i| i.time);
    let x = right.left() + 12.0;
    let width = right.width() - 24.0;
    let (title, what) = match b.tab {
        Tab::Kits => ("SAVE KIT", "the sound you hear now, as a kit in the library\n(USER bank; 8 characters, as on the LXR)"),
        Tab::Patterns => ("SAVE PATTERN", "PATTERN: the shown pattern and the sound you hear\nare added to the project folder named here\nPROJECT: every pattern and kit of the session"),
    };
    painter.text(Pos2::new(x, right.top() + 18.0), Align2::LEFT_CENTER, title, FontId::proportional(16.0), LCD_ON);
    painter.text(Pos2::new(x, right.top() + 36.0), Align2::LEFT_TOP, what, FontId::proportional(11.0), LABEL_DIM);
    painter.text(Pos2::new(x, right.top() + 104.0), Align2::LEFT_CENTER, if b.tab == Tab::Kits { "NAME" } else { "PROJECT" }, FontId::proportional(12.0), LED_RED);
    let limit = if b.tab == Tab::Kits { 8 } else { 24 };
    let field = Rect::from_min_size(Pos2::new(x, right.top() + 114.0), Vec2::new(width, 26.0));
    let resp = ui.put(field, egui::TextEdit::singleline(&mut b.save_name).char_limit(limit).font(FontId::proportional(14.0)).desired_width(width));
    if std::mem::take(&mut b.save_focus) {
        select_all(ui, &resp, b.save_name.chars().count());
    }
    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let name = b.save_name.trim().to_string();
    let mut result = None;
    match b.tab {
        Tab::Kits => {
            let slot = save::current_slot(st);
            let r = Rect::from_min_size(Pos2::new(x, right.top() + 150.0), Vec2::new(width, 28.0));
            let help = format!("also into kit slot {} of the session, which the shown pattern then uses", slot + 1);
            if (button(ui, r, "SAVE KIT", false).on_hover_text(help).clicked() || enter) && !name.is_empty() {
                result = Some(save::save_kit(st, &name, slot));
            }
        }
        Tab::Patterns => {
            let nr = st.params.seq.pending.load(Ordering::Acquire) as usize % seq::NUM_PATTERNS;
            let r = Rect::from_min_size(Pos2::new(x, right.top() + 150.0), Vec2::new(width, 28.0));
            if (button(ui, r, &format!("SAVE PATTERN P{:02}", nr + 1), false).clicked() || enter) && !name.is_empty() {
                result = Some(save::save_pattern(st, &name));
            }
            let r = Rect::from_min_size(Pos2::new(x, right.top() + 184.0), Vec2::new(width, 28.0));
            if button(ui, r, "SAVE WHOLE PROJECT", false).clicked() && !name.is_empty() {
                result = Some(save::save_project(st, &name));
            }
        }
    }
    if let Some(result) = result {
        let text = match result {
            Ok(text) => text,
            Err(e) => format!("not saved: {e}"),
        };
        b.message = Some((text, now + 5.0));
    }
    if name.is_empty() {
        painter.text(Pos2::new(x, right.top() + 232.0), Align2::LEFT_CENTER, "type a name first", FontId::proportional(12.0), LABEL_DIM);
    }
    if let Some((text, until)) = &b.message {
        if now < *until {
            painter.text(Pos2::new(x, right.top() + 232.0), Align2::LEFT_CENTER, text, FontId::proportional(12.0), LCD_ON);
        }
    }
}

/// The right column while IMPORT is lit.
fn import_form(ui: &mut Ui, st: &mut State, b: &mut BrowserState, right: Rect) {
    let painter = ui.painter().clone();
    let now = ui.input(|i| i.time);
    let x = right.left() + 12.0;
    let width = right.width() - 24.0;
    painter.text(Pos2::new(x, right.top() + 18.0), Align2::LEFT_CENTER, "IMPORT", FontId::proportional(16.0), LCD_ON);
    painter.text(
        Pos2::new(x, right.top() + 36.0),
        Align2::LEFT_TOP,
        "LXR-02 kits and projects as they come:\nErica Synths' factory content and sound\npacks (zip or unzipped folder), or a copy\nof an SD card. They are copied into your\nlibrary, under Imported.",
        FontId::proportional(11.0),
        LABEL_DIM,
    );
    let row = |n: f32| Rect::from_min_size(Pos2::new(x, right.top() + 122.0 + n * 34.0), Vec2::new(width, 28.0));
    // The chooser runs from a thread of its own: rfd hands it to the main thread, which opens it
    // once this frame is over. Opened from inside the frame, its modal loop re-enters the plugin
    // window while the window is busy drawing, and the process aborts.
    let waiting = b.picking.is_some();
    let pick = |folder: bool| {
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        let answer = slot.clone();
        std::thread::spawn(move || {
            let dialog = rfd::FileDialog::new().set_title("Import LXR-02 kits and projects");
            let picked = if folder { dialog.pick_folder() } else { dialog.add_filter("zip", &["zip", "ZIP"]).pick_file() };
            if let Ok(mut a) = answer.lock() {
                *a = Some(picked);
            }
        });
        slot
    };
    if button(ui, row(0.0), "CHOOSE A ZIP…", false).clicked() && !waiting {
        b.picking = Some(pick(false));
    }
    if button(ui, row(1.0), "CHOOSE A FOLDER…", false).clicked() && !waiting {
        b.picking = Some(pick(true));
    }
    let mut source = None;
    if let Some(slot) = &b.picking {
        match slot.lock().ok().and_then(|mut a| a.take()) {
            Some(picked) => {
                source = picked;
                b.picking = None;
            }
            None => ui.ctx().request_repaint_after(std::time::Duration::from_millis(100)),
        }
    }
    if button(ui, row(2.0), "SHOW THE LIBRARY FOLDER", false).on_hover_text("~/Music/ELABS RLX; kits and projects dropped in here show up too").clicked() {
        if let Some(root) = crate::library::root() {
            let _ = std::fs::create_dir_all(&root);
            let _ = std::process::Command::new("open").arg(root).spawn();
        }
    }
    if let Some(path) = source {
        let text = match crate::library::import(&path) {
            Ok(done) => {
                st.kits = crate::kits::all_kits();
                save::reload_projects(st);
                b.group = Some(done.name.clone()).filter(|_| b.tab == Tab::Kits && done.kits > 0);
                format!("imported {}: {} kit files, {} patterns", done.name, done.kits, done.patterns)
            }
            Err(e) => format!("not imported: {e}"),
        };
        b.message = Some((text, now + 8.0));
    }
    if let Some((text, until)) = &b.message {
        if now < *until {
            let galley = painter.layout(text.clone(), FontId::proportional(12.0), LCD_ON, width);
            painter.galley(Pos2::new(x, right.top() + 232.0), galley, LCD_ON);
        }
    }
}

pub(super) fn draw(ui: &mut Ui, setter: &ParamSetter, st: &mut State) {
    let Some(mut b) = st.browser.take() else { return };
    let close = draw_inner(ui, setter, st, &mut b);
    if !close {
        st.browser = Some(b);
    }
}

/// Returns true when the browser should close.
fn draw_inner(ui: &mut Ui, setter: &ParamSetter, st: &mut State, b: &mut BrowserState) -> bool {
    let (w, h) = (WIDTH as f32, HEIGHT as f32);
    let painter = ui.painter().clone();
    painter.rect_stroke(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h)).shrink(4.0), 10.0, Stroke::new(2.0_f32, PANEL_EDGE), StrokeKind::Inside);
    {
        let v = ui.visuals_mut();
        v.override_text_color = Some(LABEL);
        v.extreme_bg_color = Color32::from_rgb(12, 12, 13);
        v.selection.bg_fill = Color32::from_rgb(120, 30, 40);
    }

    // ---- top bar
    if button(ui, Rect::from_min_size(Pos2::new(16.0, 14.0), Vec2::new(92.0, 26.0)), "◀ PANEL", false).clicked() {
        return true;
    }
    for (i, (tab, label)) in [(Tab::Kits, "KITS"), (Tab::Patterns, "PATTERNS")].into_iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(122.0 + i as f32 * 104.0, 14.0), Vec2::new(98.0, 26.0));
        if button(ui, r, label, b.tab == tab).clicked() && b.tab != tab {
            b.tab = tab;
            b.group = None;
            b.selected = None;
            b.save_name = default_save_name(st, tab);
            b.save_focus = b.saving;
        }
    }
    let search = Rect::from_min_size(Pos2::new(336.0, 14.0), Vec2::new(240.0, 26.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(search), |ui| {
        ui.add(egui::TextEdit::singleline(&mut b.query).hint_text("search names, banks, kits").desired_width(240.0));
    });
    if button(ui, Rect::from_min_size(Pos2::new(588.0, 14.0), Vec2::new(104.0, 26.0)), "★ favorites", b.favorites_only).clicked() {
        b.favorites_only = !b.favorites_only;
    }
    if button(ui, Rect::from_min_size(Pos2::new(698.0, 14.0), Vec2::new(96.0, 26.0)), "sort by ★", b.sort_by_stars).clicked() {
        b.sort_by_stars = !b.sort_by_stars;
    }
    let import_help = "bring in LXR-02 kits and projects: Erica Synths' factory content, sound packs, an SD card copy";
    if button(ui, Rect::from_min_size(Pos2::new(w - 206.0, 14.0), Vec2::new(92.0, 26.0)), "IMPORT", b.importing).on_hover_text(import_help).clicked() {
        b.importing = !b.importing;
        b.saving = false;
    }
    let save_help = "save the current kit, the shown pattern or the whole session into the library";
    if button(ui, Rect::from_min_size(Pos2::new(w - 108.0, 14.0), Vec2::new(92.0, 26.0)), "SAVE", b.saving).on_hover_text(save_help).clicked() {
        b.saving = !b.saving;
        b.importing = false;
        b.save_name = default_save_name(st, b.tab);
        b.save_focus = b.saving;
    }

    // ---- items, filtered and sorted
    let all = items(st, b.tab);
    let query = b.query.to_lowercase();
    let mut shown: Vec<(&Item, favorites::Entry)> = all
        .iter()
        .map(|item| (item, b.favorites.get(&item.key)))
        .filter(|(item, e)| {
            (query.is_empty()
                || item.name.to_lowercase().contains(&query)
                || item.group.to_lowercase().contains(&query)
                || item.detail.to_lowercase().contains(&query)
                || e.tags.iter().any(|t| t.to_lowercase().contains(&query)))
                && (!b.favorites_only || e.stars > 0)
                && b.group.as_ref().is_none_or(|g| *g == item.group)
                && b.tag.as_ref().is_none_or(|t| e.tags.contains(t))
        })
        .collect();
    if b.sort_by_stars {
        shown.sort_by(|a, c| c.1.stars.cmp(&a.1.stars));
    }

    // ---- left: voice locks (kits), then bank / project and tag filters
    let mut top = 52.0;
    if b.tab == Tab::Kits {
        painter.text(Pos2::new(16.0, top + 6.0), Align2::LEFT_CENTER, "LOCK (keep when loading)", FontId::proportional(11.0), LED_RED);
        let locks = st.params.voice_locks.load(Ordering::Acquire);
        for (i, name) in ["D1", "D2", "D3", "SN", "CY", "HH", "FX"].iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(16.0 + (i % 4) as f32 * 45.0, top + 16.0 + (i / 4) as f32 * 26.0), Vec2::new(41.0, 22.0));
            let lit = locks & (1 << i) != 0;
            if button(ui, r, name, lit).on_hover_text("locked voices keep their sound when a kit is loaded").clicked() {
                st.params.voice_locks.fetch_xor(1 << i, Ordering::AcqRel);
            }
        }
        top += 76.0;
    }
    let left = Rect::from_min_max(Pos2::new(16.0, top), Pos2::new(196.0, h - 16.0));
    let mut groups: Vec<String> = Vec::new();
    for item in &all {
        if !groups.contains(&item.group) {
            groups.push(item.group.clone());
        }
    }
    let tags = b.favorites.all_tags();
    ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| {
        egui::ScrollArea::vertical().id_salt("filters").show(ui, |ui| {
            ui.label(egui::RichText::new(if b.tab == Tab::Kits { "BANK" } else { "PROJECT" }).color(LED_RED).size(12.0));
            if ui.selectable_label(b.group.is_none(), "all").clicked() {
                b.group = None;
            }
            for g in &groups {
                if ui.selectable_label(b.group.as_ref() == Some(g), g).clicked() {
                    b.group = if b.group.as_ref() == Some(g) { None } else { Some(g.clone()) };
                }
            }
            ui.add_space(10.0);
            ui.label(egui::RichText::new("TAG").color(LED_RED).size(12.0));
            if ui.selectable_label(b.tag.is_none(), "all").clicked() {
                b.tag = None;
            }
            for t in &tags {
                if ui.selectable_label(b.tag.as_ref() == Some(t), t).clicked() {
                    b.tag = if b.tag.as_ref() == Some(t) { None } else { Some(t.clone()) };
                }
            }
        });
    });

    // ---- middle: the list; a click loads
    let list = Rect::from_min_max(Pos2::new(208.0, 52.0), Pos2::new(690.0, h - 16.0));
    painter.rect_filled(list, 4.0, Color32::from_rgb(16, 16, 18));
    let current_kit = st.params.kit_name.lock().map(|n| n.clone()).unwrap_or_default();
    let mut to_load: Option<usize> = None;
    let mut rate: Option<(String, u8)> = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(list.shrink(6.0)), |ui| {
        egui::ScrollArea::vertical().id_salt("items").auto_shrink([false, false]).show_rows(ui, 24.0, shown.len(), |ui, range| {
            for i in range {
                let (item, entry) = &shown[i];
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
                let selected = b.selected.as_deref() == Some(item.key.as_str());
                let loaded = matches!(item.target, Target::Kit(_)) && item.name == current_kit;
                let fill = if selected {
                    Color32::from_rgb(70, 22, 28)
                } else if resp.hovered() {
                    Color32::from_rgb(34, 34, 38)
                } else {
                    Color32::TRANSPARENT
                };
                let painter = ui.painter();
                painter.rect_filled(rect, 3.0, fill);
                if let Some(r) = stars(ui, rect.left_center() + Vec2::new(6.0, 0.0), 13.0, entry.stars) {
                    rate = Some((item.key.clone(), r));
                }
                let painter = ui.painter();
                let x = rect.left() + 86.0;
                let name_color = if loaded { LCD_ON } else { LABEL };
                painter.text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, &item.name, FontId::proportional(13.0), name_color);
                let sub = if item.detail.is_empty() { item.group.clone() } else { format!("{} · kit {}", item.group, item.detail) };
                painter.text(Pos2::new(x + 120.0, rect.center().y), Align2::LEFT_CENTER, sub, FontId::proportional(11.0), LABEL_DIM);
                let tags: Vec<&str> = entry.tags.iter().map(String::as_str).take(3).collect();
                painter.text(rect.right_center() - Vec2::new(6.0, 0.0), Align2::RIGHT_CENTER, tags.join(" · "), FontId::proportional(10.5), Color32::from_rgb(220, 140, 150));
                if resp.clicked() {
                    to_load = Some(i);
                }
            }
        });
    });
    painter.text(
        Pos2::new(list.center().x, h - 8.0),
        Align2::CENTER_CENTER,
        format!("{} of {}", shown.len(), all.len()),
        FontId::proportional(10.0),
        LABEL_DIM,
    );
    let selected_target = to_load.map(|i| (shown[i].0.key.clone(), match shown[i].0.target {
        Target::Kit(k) => Target::Kit(k),
        Target::Pattern { project, nr } => Target::Pattern { project, nr },
    }));

    // ---- right: the selected item's rating and tags
    let right = Rect::from_min_max(Pos2::new(702.0, 52.0), Pos2::new(w - 16.0, h - 16.0));
    painter.rect_filled(right, 4.0, Color32::from_rgb(16, 16, 18));
    let selected_item = b.selected.as_ref().and_then(|k| all.iter().find(|i| &i.key == k)).filter(|_| !b.saving && !b.importing);
    if b.saving {
        save_form(ui, st, b, right);
    }
    if b.importing {
        import_form(ui, st, b, right);
    }
    match selected_item {
        Some(item) => {
            let entry = b.favorites.get(&item.key);
            painter.text(right.min + Vec2::new(12.0, 18.0), Align2::LEFT_CENTER, &item.name, FontId::proportional(18.0), LCD_ON);
            let sub = if item.detail.is_empty() { item.group.clone() } else { format!("{} · kit {}", item.group, item.detail) };
            painter.text(right.min + Vec2::new(12.0, 40.0), Align2::LEFT_CENTER, sub, FontId::proportional(11.5), LABEL_DIM);
            if let Some(r) = stars(ui, right.min + Vec2::new(12.0, 68.0), 26.0, entry.stars) {
                rate = Some((item.key.clone(), r));
            }
            if matches!(item.target, Target::Pattern { .. }) && !st.params.seq_on.value() {
                let r = Rect::from_min_size(right.min + Vec2::new(12.0, 92.0), Vec2::new(right.width() - 24.0, 24.0));
                if button(ui, r, "PLAY is off: turn the sequencer on", false).clicked() {
                    setter.begin_set_parameter(&st.params.seq_on);
                    setter.set_parameter(&st.params.seq_on, true);
                    setter.end_set_parameter(&st.params.seq_on);
                }
            }
            painter.text(right.min + Vec2::new(12.0, 132.0), Align2::LEFT_CENTER, "TAGS", FontId::proportional(12.0), LED_RED);
            let tag_area = Rect::from_min_max(right.min + Vec2::new(10.0, 144.0), right.max - Vec2::new(10.0, 44.0));
            let mut toggle: Option<String> = None;
            ui.scope_builder(egui::UiBuilder::new().max_rect(tag_area), |ui| {
                egui::ScrollArea::vertical().id_salt("tags").show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for t in &tags {
                            if ui.selectable_label(entry.tags.contains(t), t).clicked() {
                                toggle = Some(t.clone());
                            }
                        }
                    });
                });
            });
            let new_row = Rect::from_min_max(Pos2::new(right.left() + 10.0, right.bottom() - 36.0), right.max - Vec2::new(10.0, 8.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(new_row), |ui| {
                ui.horizontal(|ui| {
                    let edit = ui.add(egui::TextEdit::singleline(&mut b.new_tag).hint_text("new tag").desired_width(right.width() - 90.0));
                    let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (ui.button("add").clicked() || enter) && !b.new_tag.trim().is_empty() {
                        toggle = Some(b.new_tag.trim().to_string());
                        b.new_tag.clear();
                    }
                });
            });
            if let Some(t) = toggle {
                b.favorites.toggle_tag(&item.key, &t);
            }
        }
        None if b.saving || b.importing => {}
        None => {
            painter.text(
                right.center(),
                Align2::CENTER_CENTER,
                "click an item to load it\nand rate or tag it here",
                FontId::proportional(13.0),
                LABEL_DIM,
            );
        }
    }

    if let Some((key, r)) = rate {
        b.favorites.set_stars(&key, r);
    }
    if let Some((key, target)) = selected_target {
        b.selected = Some(key);
        load(st, setter, &target);
    }
    false
}
