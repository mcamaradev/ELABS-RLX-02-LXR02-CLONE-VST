//! Front panel GUI modelled on the LXR-02's layout: the 2x16 LCD shows four parameters of the
//! selected voice and sub page (as defined by the firmware's menuPages), the four knobs below it
//! edit them, DATA edits the selected one in single steps, and the faders are the voice levels.

use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use nih_plug_egui::{create_egui_editor, EguiState};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::kits::{self, Kit};
use crate::param_table::Display;
use crate::ui_pages::{Slot, SubPage, FX_PAGES, MOD_TARGETS, VOICE_PAGES};
use crate::{LxrParams, UiShared, FX_TYPE_PARAM};

mod all_params;
mod browser;
mod history;
mod lane_detail;
mod maestro_panel;
mod entropia_view;
mod lanes;
mod save;
mod sequencer;
mod song;

/// Panel coordinates are taken from a 520x262 photo of the unit and scaled up.
const S: f32 = 1.92;
/// name on the panel's name plate
const NAME: &str = "ELABS RLX";
/// model name, bottom left
const MODEL: &str = "RLX-02";
pub const WIDTH: u32 = (520.0 * S) as u32;
pub const HEIGHT: u32 = (262.0 * S) as u32;

const PANEL: Color32 = Color32::from_rgb(24, 24, 26);
const PANEL_EDGE: Color32 = Color32::from_rgb(48, 48, 52);
const LABEL: Color32 = Color32::from_rgb(200, 200, 204);
const LABEL_DIM: Color32 = Color32::from_rgb(110, 110, 116);
const BUTTON: Color32 = Color32::from_rgb(112, 112, 118);
const BUTTON_DIM: Color32 = Color32::from_rgb(70, 70, 75);
const LED_RED: Color32 = Color32::from_rgb(255, 82, 138);
const LCD_BG: Color32 = Color32::from_rgb(22, 4, 4);
const LCD_OFF: Color32 = Color32::from_rgb(48, 10, 9);
const LCD_ON: Color32 = Color32::from_rgb(255, 46, 30);

const VOICE_LABELS: [&str; 7] = ["DRUM1", "DRUM2", "DRUM3", "SNARE", "CLP/CYM", "CL HH", "OP HH"];
const FADER_LABELS: [&str; 7] = ["DRUM1", "DRUM2", "DRUM3", "SNARE", "CLP/CYM", "HIHATS", "VOLUME"];
const SUB_LABELS: [&str; 8] = ["OSC", "AEG", "MOD", "FM", "CLICK", "FILTER", "LFO", "MIX"];
const SUB_HELP: [&str; 8] = [
    "OSC: oscillator (pitch, waveform)\npress again for the second row",
    "AEG: amplitude envelope",
    "MOD: pitch envelope and velocity modulation",
    "FM: frequency modulation",
    "CLICK: transient (attack sample)",
    "FILTER",
    "LFO: rate, depth, destination\npress again for the second row",
    "MIX: volume, pan, drive, output, track length",
];
const STEP_LABELS: [&str; 16] = [
    "DRUM1", "DRUM2", "DRUM3", "SNARE", "CLP/CYM", "CL HH", "OP HH", "KIT", "MORPH", "SONG",
    "PROJECT", "CONFIG", "RELOAD", "LAST STEP", "FX", "PATTERN",
];
const STEP_HELP: [&str; 16] = [
    "play DRUM1", "play DRUM2", "play DRUM3", "play SNARE", "play CLP/CYM", "play CL HH", "play OP HH",
    "KIT: kit browser on the display", "MORPH: not in this plugin (morph is on the LOAD page)", "SONG: song mode",
    "PROJECT: use PATTERN or the BROWSER", "CONFIG: not in this plugin", "RELOAD: not in this plugin",
    "LAST STEP: track length is \"len\" on the MIX page", "FX: master effect page", "PATTERN: pattern and project browser",
];
const STEP_KIT: usize = 7;
const STEP_SONG: usize = 9;
// firmware parameter numbers of the modulation destination settings
const PAR_VEL_DEST_1: u16 = 149;
const PAR_VOICE_LFO1: u16 = 161;
const PAR_TARGET_LFO1: u16 = 167;
const STEP_FX: usize = 14;
static NO_PARAMS: SubPage = [None; 8];
const COLUMN_X: [f32; 7] = [307.0, 338.5, 370.0, 402.0, 434.0, 465.5, 497.0];
/// the four knobs, evenly spread from the LCD's left edge to its right edge (40-210)
const KNOB_X: [f32; 4] = [40.0, 96.67, 153.33, 210.0];

/// Focuses a text field with its whole text selected, so typing replaces it.
fn select_all(ui: &Ui, resp: &Response, len: usize) {
    resp.request_focus();
    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), resp.id) {
        let range = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(len));
        state.cursor.set_char_range(Some(range));
        state.store(ui.ctx(), resp.id);
    }
}

fn p(x: f32, y: f32) -> Pos2 {
    Pos2::new(x * S, y * S)
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Voice(usize),
    Fx,
    /// kit browser (LOAD / KIT)
    Load,
    /// pattern and project browser (PATTERN)
    Pattern,
    /// euclid generator and shuffle (PERF)
    Perf,
    /// save a kit or the session as a project (SAVE)
    Save,
    /// settings of the selected step (right-click a step)
    Step,
    /// clear and copy tracks and patterns (COPY/CLR)
    Copy,
    /// song mode list (SONG)
    Song,
}

struct State {
    params: Arc<LxrParams>,
    shared: Arc<UiShared>,
    page: Page,
    sub: usize,
    /// which half of the sub page (slots 0-3 or 4-7) the knobs show
    half: usize,
    /// slot edited by the DATA encoder (0-7)
    active: usize,
    led_until: [f64; 7],
    drag_accum: f32,
    scroll_accum: f32,
    kits: Vec<Kit>,
    /// kit selected in the browser (index into `kits`)
    kit_index: usize,
    /// voice page to return to when leaving the browser
    last_page: Page,
    /// tab of the ALL PARAMS view while it is shown
    all_tab: Option<usize>,
    /// SHIFT latched: the step buttons show their printed functions instead of steps
    shift: bool,
    /// bar (0-3) the step buttons show
    bar: usize,
    /// voice track the step buttons edit when no voice page is shown
    last_track: usize,
    /// built-in and library projects, for the pattern page
    projects: Vec<crate::seq::Project>,
    /// pattern page selection
    proj_sel: usize,
    pat_sel: usize,
    /// last active pattern seen, to load the next pattern's kit
    last_active: Option<u8>,
    /// value the DATA encoder edits on the PERF page (0-3)
    perf_col: usize,
    save: save::SaveState,
    /// the kit browser loads into the morph slot (kit B) instead of the kit
    load_to_morph: bool,
    /// step being edited (0-63 in the shown pattern, on the current track), its view and column
    sel_step: Option<usize>,
    step_view: usize,
    step_col: usize,
    copy: song::CopyState,
    song_cursor: usize,
    /// the preset browser while it is open
    browser: Option<browser::BrowserState>,
    /// the LANES view is open
    lanes_open: bool,
    /// the ENTROPIA MOD view (FX while the FX mode is ENTROPIA MOD)
    entropia_open: bool,
    /// while painting steps in LANES: whether the stroke turns them on
    lane_paint: Option<bool>,
    /// LANES detail strip: which setting it shows, the parameter of each lock slot, and the last
    /// step and value drawn in the stroke under way
    lane_detail: usize,
    lane_lock: [Option<u16>; 2],
    lane_draw: Option<(usize, i32)>,
    /// the Maestro's panel shows instead of the LANES detail strip
    maestro_open: bool,
    /// value shown next to the control being turned this frame
    popup: Option<(Pos2, String)>,
    history: history::History,
    /// the ELABS logo, made into a texture each time the window opens
    logo: Option<egui::TextureHandle>,
}

impl State {
    fn sub_page(&self) -> &'static SubPage {
        match self.page {
            Page::Voice(v) => &VOICE_PAGES[v][self.sub],
            Page::Fx => {
                let fx = self.params.by_index(FX_TYPE_PARAM).map_or(0, |p| p.value()) as usize;
                &FX_PAGES[fx.min(FX_PAGES.len() - 1)]
            }
            Page::Load | Page::Pattern | Page::Perf | Page::Save | Page::Step | Page::Copy | Page::Song => &NO_PARAMS,
        }
    }

    /// Opens a browser page, or goes back to the voice/FX page if it is already open.
    fn toggle_page(&mut self, page: Page) {
        if self.page == page {
            self.page = self.last_page;
        } else {
            if matches!(self.page, Page::Voice(_) | Page::Fx) {
                self.last_page = self.page;
            }
            self.page = page;
        }
    }

    /// first and one-past-last kit index of the selected kit's bank
    fn bank_range(&self) -> (usize, usize) {
        let bank = self.kits.get(self.kit_index).map(|k| k.bank.as_str()).unwrap_or("");
        let start = self.kits.iter().position(|k| k.bank == bank).unwrap_or(0);
        let end = start + self.kits[start..].iter().take_while(|k| k.bank == bank).count();
        (start, end.max(start + 1))
    }

    fn bank_starts(&self) -> Vec<usize> {
        (0..self.kits.len()).filter(|&i| i == 0 || self.kits[i].bank != self.kits[i - 1].bank).collect()
    }

    fn toggle_load_page(&mut self) {
        self.toggle_page(Page::Load);
    }

    fn load_selected(&mut self, setter: &ParamSetter) {
        if let Some(kit) = self.kits.get(self.kit_index) {
            if self.load_to_morph {
                self.params.set_morph_kit(Some((kit.name.clone(), kit.data.clone())));
            } else {
                kits::load(setter, &self.params, kit);
            }
        }
    }

    fn slot(&self, column: usize) -> Option<Slot> {
        self.sub_page()[self.half * 4 + column]
    }

    fn select_sub(&mut self, sub: usize) {
        if let Page::Voice(v) = self.page {
            if VOICE_PAGES[v][sub][0].is_none() {
                return;
            }
        }
        if self.sub == sub && self.page != Page::Fx {
            // pressing the selected sub page again shows its second half, like on the unit
            let has_second = self.sub_page()[4..].iter().any(Option::is_some);
            self.half = if has_second { 1 - self.half } else { 0 };
        } else {
            self.sub = sub;
            self.half = 0;
        }
        self.active = self.half * 4;
    }
}

pub fn default_state() -> Arc<EguiState> {
    EguiState::from_size(WIDTH, HEIGHT)
}

pub fn create(params: Arc<LxrParams>, shared: Arc<UiShared>) -> Option<Box<dyn Editor>> {
    let kits = kits::all_kits();
    let projects = crate::seq::all_projects();
    let current = params.kit_name.lock().map(|n| n.clone()).unwrap_or_default();
    let kit_index = kits.iter().position(|k| k.name == current).unwrap_or(0);
    let state = State {
        kits,
        kit_index,
        last_page: Page::Voice(0),
        all_tab: None,
        shift: false,
        bar: 0,
        last_track: 0,
        proj_sel: params.seq.project_name().and_then(|n| projects.iter().position(|p| p.name == n)).unwrap_or(0),
        projects,
        pat_sel: params.seq.pending.load(Ordering::Acquire) as usize % crate::seq::NUM_PATTERNS,
        last_active: None,
        perf_col: 0,
        save: save::SaveState::default(),
        load_to_morph: false,
        sel_step: None,
        step_view: 0,
        step_col: 0,
        copy: song::CopyState::default(),
        song_cursor: 0,
        browser: None,
        lanes_open: false,
        entropia_open: false,
        lane_paint: None,
        lane_detail: 0,
        lane_lock: [None; 2],
        lane_draw: None,
        maestro_open: false,
        popup: None,
        history: history::History::default(),
        logo: None,
        params: params.clone(),
        shared,
        page: Page::Voice(0),
        sub: 0,
        half: 0,
        active: 0,
        led_until: [0.0; 7],
        drag_accum: 0.0,
        scroll_accum: 0.0,
    };
    // the build step runs each time the window opens, with a new egui context: textures made for
    // an earlier window are gone, so the logo is loaded again here
    create_egui_editor(params.editor_state.clone(), state, |ctx, state| state.logo = load_logo(ctx), |ctx, setter, state| {
        // the panel is drawn at its base size on a layer scaled to the window (egui-baseview
        // ignores egui's own zoom factor); the layer transform scales the mouse input too
        let zoom = zoom(&state.params.editor_state);
        history::begin_frame(ctx, setter, state);
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(PANEL)).show(ctx, |_| {});
        let id = egui::Id::new("lxr-panel");
        ctx.set_transform_layer(egui::LayerId::new(egui::Order::Middle, id), egui::emath::TSTransform::from_scaling(zoom));
        egui::Area::new(id).order(egui::Order::Middle).fixed_pos(Pos2::ZERO).constrain(false).show(ctx, |ui| {
            let base = Rect::from_min_size(Pos2::ZERO, Vec2::new(WIDTH as f32, HEIGHT as f32));
            ui.set_clip_rect(base);
            ui.set_min_size(base.size());
            ui.painter().rect_filled(base, 0.0, PANEL);
            state.popup = None;
            draw_panel(ui, setter, state);
            draw_popup(ui, state);
            history::draw_message(ui, state);
            resize_corner(ui, state, zoom);
        });
        ctx.request_repaint();
    })
}

/// The ELABS logo (Entropia sticker), decoded from the PNG built into the plugin.
fn load_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let mut decoder = png::Decoder::new(&include_bytes!("../assets/logo.png")[..]);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let pixels = &buf[..info.buffer_size()];
    let size = [info.width as usize, info.height as usize];
    let image = match info.color_type {
        png::ColorType::Rgba => egui::ColorImage::from_rgba_unmultiplied(size, pixels),
        png::ColorType::Rgb => egui::ColorImage::from_rgb(size, pixels),
        _ => return None,
    };
    Some(ctx.load_texture("elabs-logo", image, egui::TextureOptions::LINEAR))
}

/// The value of the control being turned, above it and over everything else.
fn draw_popup(ui: &mut Ui, st: &State) {
    let Some((anchor, text)) = &st.popup else { return };
    let font = FontId::proportional(13.0);
    let galley = ui.painter().layout_no_wrap(text.clone(), font, Color32::WHITE);
    let size = galley.size() + Vec2::new(16.0, 8.0);
    let mut rect = Rect::from_center_size(*anchor - Vec2::new(0.0, size.y / 2.0), size);
    // keep it inside the panel
    let panel = Rect::from_min_size(Pos2::ZERO, Vec2::new(WIDTH as f32, HEIGHT as f32)).shrink(4.0);
    rect = rect.translate(Vec2::new(
        (panel.left() - rect.left()).max(0.0) + (panel.right() - rect.right()).min(0.0),
        (panel.top() - rect.top()).max(0.0),
    ));
    let painter = ui.painter();
    painter.rect_filled(rect.translate(Vec2::new(0.0, 2.0)), 5.0, Color32::from_black_alpha(140));
    painter.rect_filled(rect, 5.0, Color32::from_rgb(30, 8, 10));
    painter.rect_stroke(rect, 5.0, Stroke::new(1.0_f32, LED_RED), StrokeKind::Inside);
    painter.galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
}

/// Whether a control is being turned this frame (dragged, or scrolled while hovered).
fn turning(ui: &Ui, resp: &Response) -> bool {
    resp.dragged() || (resp.hovered() && ui.input(|i| i.raw_scroll_delta.y != 0.0))
}

/// Window sizes offered as presets, in percent of the base size.
const SIZES: [u32; 5] = [75, 100, 125, 150, 200];

fn zoom(editor_state: &EguiState) -> f32 {
    let (w, h) = editor_state.size();
    (w as f32 / WIDTH as f32).min(h as f32 / HEIGHT as f32).clamp(0.25, 4.0)
}

fn request_scale(editor_state: &EguiState, scale: f32) {
    let scale = scale.clamp(0.5, 2.5);
    editor_state.set_requested_size(((WIDTH as f32 * scale).round() as u32, (HEIGHT as f32 * scale).round() as u32));
}

/// Dragging the bottom right corner resizes the window, keeping the panel's proportions.
fn resize_corner(ui: &mut Ui, st: &mut State, zoom: f32) {
    let corner = Rect::from_min_size(Pos2::new(WIDTH as f32, HEIGHT as f32) - Vec2::splat(18.0), Vec2::splat(18.0));
    let resp = ui.interact(corner, egui::Id::new("resize-corner"), Sense::drag());
    let color = if resp.hovered() || resp.dragged() { LABEL } else { LABEL_DIM };
    for i in 1..4 {
        let d = i as f32 * 4.5;
        ui.painter().line_segment([corner.max - Vec2::new(d, 2.0), corner.max - Vec2::new(2.0, d)], Stroke::new(1.2_f32, color));
    }
    if resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            // the pointer is in panel coordinates; the window is the panel times the zoom
            let scale = (pos.x * zoom / WIDTH as f32).max(pos.y * zoom / HEIGHT as f32);
            request_scale(&st.params.editor_state, scale);
        }
    }
    resp.on_hover_text("drag to resize the window");
}

fn draw_panel(ui: &mut Ui, setter: &ParamSetter, st: &mut State) {
    let now = ui.input(|i| i.time);
    let fired = st.shared.fired.swap(0, Ordering::AcqRel);
    for v in 0..7 {
        if fired & (1 << v) != 0 {
            st.led_until[v] = now + 0.12;
        }
    }

    sequencer::follow_kit(setter, st);
    if let Page::Voice(v) = st.page {
        st.last_track = v;
    }
    let step_mode = st.params.seq_on.value() && !st.shift;

    if st.browser.is_some() {
        browser::draw(ui, setter, st);
        return;
    }
    if st.lanes_open {
        if lanes::draw(ui, setter, st) {
            st.lanes_open = false;
        }
        return;
    }
    if st.entropia_open {
        if entropia_view::draw(ui, setter, st) {
            st.entropia_open = false;
        }
        return;
    }
    if let Some(tab) = st.all_tab {
        all_params::draw(ui, setter, st, tab);
        return;
    }

    let painter = ui.painter().clone();
    painter.rect_stroke(
        Rect::from_min_max(p(2.0, 2.0), p(518.0, 260.0)),
        10.0,
        Stroke::new(2.0_f32, PANEL_EDGE),
        StrokeKind::Inside,
    );

    // name plate (no manufacturer logos): the name in heavy letters and the ELABS logo where the
    // LXR-02 has its mascot
    let title = painter.layout_no_wrap(NAME.to_string(), FontId::proportional(13.5 * S), Color32::WHITE);
    let logo_size = 30.0 * S;
    let gap = 1.5 * S;
    let left = p(110.0, 0.0).x - (title.size().x + gap + logo_size) / 2.0;
    let baseline = p(0.0, 23.0).y;
    let title_pos = Pos2::new(left, baseline - title.size().y / 2.0);
    // drawn three times, a hair apart, for a bold face
    for offset in [Vec2::ZERO, Vec2::new(0.8, 0.0), Vec2::new(0.4, 0.4)] {
        painter.galley(title_pos + offset, title.clone(), Color32::WHITE);
    }
    if let Some(logo) = &st.logo {
        let rect = Rect::from_min_size(Pos2::new(left + title.size().x + gap, baseline - logo_size * 0.64), Vec2::splat(logo_size));
        painter.image(logo.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        ui.allocate_rect(rect, Sense::hover()).on_hover_text("ELABS · Entropia Labs");
    }
    painter.text(p(110.0, 40.5), Align2::CENTER_CENTER, "DIGITAL DRUM SYNTH", FontId::proportional(5.2 * S), Color32::WHITE);

    // opens the ALL PARAMS view on the selected voice
    let all = Rect::from_center_size(p(188.0, 20.0), Vec2::new(30.0 * S, 9.0 * S));
    let resp = ui.allocate_rect(all, Sense::click()).on_hover_text("every parameter of a voice on one page\n(with Live's Configure on, touched controls can be MIDI mapped)");
    painter.rect_stroke(all, 3.0, Stroke::new(1.0_f32, if resp.hovered() { LABEL } else { LABEL_DIM }), StrokeKind::Inside);
    painter.text(all.center(), Align2::CENTER_CENTER, "ALL PARAMS", FontId::proportional(4.2 * S), LABEL);
    if resp.clicked() {
        st.all_tab = Some(match st.page {
            Page::Voice(v) => v.min(5),
            _ => 6,
        });
    }
    // opens the preset browser (the display does too, on the voice and FX pages)
    let browse = Rect::from_center_size(p(188.0, 31.0), Vec2::new(30.0 * S, 9.0 * S));
    let resp = ui.allocate_rect(browse, Sense::click()).on_hover_text("preset browser: kits and patterns, stars and tags");
    painter.rect_stroke(browse, 3.0, Stroke::new(1.0_f32, if resp.hovered() { LABEL } else { LABEL_DIM }), StrokeKind::Inside);
    painter.text(browse.center(), Align2::CENTER_CENTER, "BROWSER", FontId::proportional(4.2 * S), LABEL);
    if resp.clicked() {
        st.browser = Some(browser::BrowserState::new(false));
    }
    // opens the LANES view: every track of the pattern at once
    let lanes = Rect::from_center_size(p(188.0, 42.0), Vec2::new(30.0 * S, 9.0 * S));
    let resp = ui.allocate_rect(lanes, Sense::click()).on_hover_text("all seven tracks of the pattern at once");
    painter.rect_stroke(lanes, 3.0, Stroke::new(1.0_f32, if resp.hovered() { LABEL } else { LABEL_DIM }), StrokeKind::Inside);
    painter.text(lanes.center(), Align2::CENTER_CENTER, "LANES", FontId::proportional(4.2 * S), LABEL);
    if resp.clicked() {
        st.lanes_open = true;
    }
    if st.page == Page::Fx {
        fx_mode_switch(ui, setter, st);
    }
    // model name in wide-set letters, where the unit has "LXR-02"
    for (k, c) in MODEL.chars().enumerate() {
        painter.text(p(17.0 + k as f32 * 6.2, 246.0), Align2::CENTER_CENTER, c, FontId::proportional(7.0 * S), LABEL);
    }
    painter.text(p(250.0, 246.0), Align2::CENTER_CENTER, "GPL-3.0 FIRMWARE BUILD", FontId::proportional(4.5 * S), LABEL_DIM);

    history::buttons(
        ui,
        setter,
        st,
        [Rect::from_center_size(p(318.0, 246.0), Vec2::new(19.0 * S, 7.5 * S)), Rect::from_center_size(p(340.0, 246.0), Vec2::new(19.0 * S, 7.5 * S))],
        3.8 * S,
    );

    // window size presets
    painter.text(p(400.0, 246.0), Align2::RIGHT_CENTER, "SIZE", FontId::proportional(4.2 * S), LABEL_DIM);
    let current = (zoom(&st.params.editor_state) * 100.0).round() as u32;
    for (i, size) in SIZES.iter().enumerate() {
        let r = Rect::from_center_size(p(412.0 + i as f32 * 20.0, 246.0), Vec2::new(17.0 * S, 7.5 * S));
        let resp = ui.allocate_rect(r, Sense::click());
        let lit = current == *size;
        painter.rect_filled(r, 2.0, if lit { LED_RED } else if resp.hovered() { BUTTON } else { BUTTON_DIM });
        painter.text(r.center(), Align2::CENTER_CENTER, format!("{size}%"), FontId::proportional(3.8 * S), if lit { PANEL } else { LABEL });
        if resp.clicked() {
            request_scale(&st.params.editor_state, *size as f32 / 100.0);
        }
    }

    draw_lcd(ui, setter, st);
    draw_encoder(ui, setter, st);
    for column in 0..4 {
        draw_knob(ui, setter, st, column);
    }
    draw_faders(ui, setter, st);

    // BAR arrows (above SAVE and OSC, like on the unit) pick the bar the step buttons show; without the sequencer they step through the
    // halves of the sub page
    let left = round_button(ui, p(245.0, 148.0), false, true)
        .on_hover_text("BAR ◀: previous bar of the steps (sequencer on)\nfirst row of the page (sequencer off) · step views on the step page")
        .clicked();
    let right = round_button(ui, p(275.0, 148.0), false, true)
        .on_hover_text("BAR ▶: next bar of the steps (sequencer on)\nsecond row of the page (sequencer off) · step views on the step page")
        .clicked();
    if st.page == Page::Step {
        if left {
            sequencer::step_view(st, -1);
        }
        if right {
            sequencer::step_view(st, 1);
        }
    } else if step_mode {
        if left {
            st.bar = st.bar.saturating_sub(1);
        }
        if right {
            st.bar = (st.bar + 1).min(3);
        }
        painter.text(p(260.0, 140.0), Align2::CENTER_CENTER, format!("BAR {}/4", st.bar + 1), FontId::proportional(4.2 * S), LED_RED);
    } else {
        if left && st.half == 1 {
            st.half = 0;
            st.active = 0;
        }
        if right && st.sub_page()[4..].iter().any(Option::is_some) && st.half == 0 {
            st.half = 1;
            st.active = 4;
        }
    }
    small_label(&painter, p(245.0, 158.0), "◀");
    small_label(&painter, p(260.0, 158.0), "BAR");
    small_label(&painter, p(275.0, 158.0), "▶");

    // voice select buttons next to the faders
    for v in 0..7 {
        // on the PERF page they pick the track the euclid generator works on
        let muted = st.params.seq.mutes.load(Ordering::Acquire) & (1 << v) != 0;
        let lit = if st.shift {
            !muted
        } else {
            st.page == Page::Voice(v) || (matches!(st.page, Page::Perf | Page::Step) && st.last_track == v)
        };
        if muted {
            painter.text(p(COLUMN_X[v], 139.5), Align2::CENTER_CENTER, "MUTE", FontId::proportional(3.6 * S), LED_RED);
        }
        // voice locked against kit loading (the open hihat shares the hihat's sound)
        if st.params.voice_locks.load(Ordering::Acquire) & (1 << v.min(5)) != 0 {
            painter.text(p(COLUMN_X[v], 134.5), Align2::CENTER_CENTER, "LOCK", FontId::proportional(3.6 * S), Color32::from_rgb(255, 200, 60));
        }
        if round_button(ui, p(COLUMN_X[v], 148.0), lit, true)
            .on_hover_text(format!("{}: select the voice\nSHIFT + click: mute its sequencer track", VOICE_LABELS[v]))
            .clicked()
        {
            if st.shift {
                // SHIFT + voice mutes the voice's sequencer track
                st.params.seq.mutes.fetch_xor(1 << v, Ordering::AcqRel);
            } else if matches!(st.page, Page::Perf | Page::Step) {
                st.last_track = v;
            } else {
                st.page = Page::Voice(v);
                if VOICE_PAGES[v][st.sub][0].is_none() {
                    st.sub = 0;
                }
                st.half = 0;
                st.active = 0;
            }
        }
        small_label(&painter, p(COLUMN_X[v], 158.0), VOICE_LABELS[v]);
    }

    // function row
    group_box(&painter, 8.0, 140.0);
    group_box(&painter, 140.0, 262.0);
    group_box(&painter, 262.0, 512.0);
    for (i, label) in ["SHIFT", "PLAY", "REC", "COPY/CLR"].iter().enumerate() {
        let center = p(22.0 + i as f32 * 32.0, 177.0);
        match i {
            0 => {
                if round_button(ui, center, st.shift, true)
                    .on_hover_text("SHIFT (latching): the step buttons show their printed functions\nSHIFT + voice button: mute that track")
                    .clicked()
                {
                    st.shift = !st.shift;
                }
            }
            // turns the internal sequencer on and off; it plays while the host transport runs
            1 => {
                let on = st.params.seq_on.value();
                if round_button(ui, center, on, true)
                    .on_hover_text("PLAY: the internal sequencer on/off\nit plays while the host transport runs")
                    .clicked()
                {
                    setter.begin_set_parameter(&st.params.seq_on);
                    setter.set_parameter(&st.params.seq_on, !on);
                    setter.end_set_parameter(&st.params.seq_on);
                }
            }
            // live recording: pads and MIDI notes are written into the playing pattern
            2 => {
                let rec = st.params.seq.record.load(Ordering::Acquire);
                if round_button(ui, center, rec, true)
                    .on_hover_text("REC: pads and MIDI notes are recorded into the playing pattern")
                    .clicked()
                {
                    st.params.seq.record.store(!rec, Ordering::Release);
                }
            }
            _ => {
                if round_button(ui, center, st.page == Page::Copy, true)
                    .on_hover_text("COPY/CLR: clear or copy tracks and patterns, double or halve a pattern")
                    .clicked()
                {
                    st.toggle_page(Page::Copy);
                }
            }
        }
        match i {
            0 => boxed_label(&painter, p(22.0, 187.5), Align2::CENTER_CENTER, label),
            3 => {
                // "COPY" with "CLR" boxed, like the unit's SHIFT function
                let at = p(22.0 + 3.0 * 32.0, 187.5);
                painter.text(at - Vec2::new(1.0 * S, 0.0), Align2::RIGHT_CENTER, "COPY", FontId::proportional(4.2 * S), LABEL);
                boxed_label(&painter, at + Vec2::new(1.0 * S, 0.0), Align2::LEFT_CENTER, "CLR");
            }
            _ => small_label(&painter, p(22.0 + i as f32 * 32.0, 187.5), label),
        }
    }
    for (i, label) in ["VOICE", "PERF", "LOAD", "SAVE"].iter().enumerate() {
        let center = p(152.0 + i as f32 * 31.0, 177.0);
        match i {
            0 => {
                let browsing = !matches!(st.page, Page::Voice(_) | Page::Fx);
                if round_button(ui, center, !browsing, true).on_hover_text("VOICE: back to the sound pages").clicked() && browsing {
                    st.page = st.last_page;
                }
            }
            1 => {
                if round_button(ui, center, st.page == Page::Perf, true)
                    .on_hover_text("PERF: euclid generator and shuffle")
                    .clicked()
                {
                    st.toggle_page(Page::Perf);
                }
            }
            2 => {
                if round_button(ui, center, st.page == Page::Load, true)
                    .on_hover_text("LOAD: kit browser on the display (and the morph kit B)")
                    .clicked()
                {
                    st.toggle_load_page();
                }
            }
            _ => {
                if round_button(ui, center, st.page == Page::Save, true)
                    .on_hover_text("SAVE: save the kit or the whole session as a project")
                    .clicked()
                {
                    st.toggle_page(Page::Save);
                    if st.page == Page::Save {
                        save::open(st);
                    }
                }
            }
        }
        small_label(&painter, p(152.0 + i as f32 * 31.0, 187.5), label);
    }
    for (i, label) in SUB_LABELS.iter().enumerate() {
        let available = match st.page {
            Page::Voice(v) => VOICE_PAGES[v][i][0].is_some(),
            _ => false,
        };
        let lit = matches!(st.page, Page::Voice(_)) && st.sub == i;
        if round_button(ui, p(275.0 + i as f32 * 31.7, 177.0), lit, available).on_hover_text(SUB_HELP[i]).clicked() {
            st.select_sub(i);
        }
        small_label(&painter, p(275.0 + i as f32 * 31.7, 187.5), label);
    }

    // step buttons: the selected voice's steps while the sequencer is on (unless SHIFT is latched);
    // otherwise their printed functions: 1-7 play the voices, KIT, FX and PATTERN open their pages
    if step_mode {
        sequencer::draw_steps(ui, st);
        return;
    }
    for i in 0..16 {
        let center = p(25.0 + i as f32 * 31.4, 209.0);
        let (lit, enabled) = match i {
            0..=6 => (now < st.led_until[i], true),
            STEP_KIT => (st.page == Page::Load, true),
            STEP_FX => (st.page == Page::Fx, true),
            sequencer::STEP_PATTERN => (st.page == Page::Pattern, true),
            STEP_SONG => (st.page == Page::Song, true),
            _ => (false, false),
        };
        let resp = step_button(ui, center, lit, enabled, i % 4 == 0).on_hover_text(STEP_HELP[i]);
        // pads fire on press, not on release
        if i < 7 && resp.hovered() && ui.input(|inp| inp.pointer.primary_pressed()) {
            st.shared.pending.fetch_or(1 << i, Ordering::AcqRel);
            st.led_until[i] = now + 0.12;
        }
        if i == STEP_KIT && resp.clicked() {
            st.toggle_load_page();
        }
        if i == sequencer::STEP_PATTERN && resp.clicked() {
            st.toggle_page(Page::Pattern);
        }
        if i == STEP_SONG && resp.clicked() {
            st.toggle_page(Page::Song);
        }
        if i == STEP_FX && resp.clicked() && st.params.fx_mode.value() {
            st.entropia_open = true;
        } else if i == STEP_FX && resp.clicked() {
            if st.page == Page::Fx {
                st.page = Page::Voice(0);
                st.sub = 0;
            } else {
                st.page = Page::Fx;
            }
            st.half = 0;
            st.active = 0;
        }
        let label_at = center + Vec2::new(0.0, 19.0 * S * 0.5 + 4.5 * S);
        if i >= 11 {
            // the unit prints its second functions of these in white boxes
            boxed_label(&painter, label_at, Align2::CENTER_CENTER, STEP_LABELS[i]);
        } else {
            small_label(&painter, label_at, STEP_LABELS[i]);
        }
    }
}

/// FX page: the LXR's own FX bus (CLASSIC, what the LCD edits) or ENTROPIA MOD (its own view).
fn fx_mode_switch(ui: &mut Ui, setter: &ParamSetter, st: &mut State) {
    let painter = ui.painter().clone();
    painter.text(p(40.0, 153.0), Align2::CENTER_CENTER, "FX MODE", FontId::proportional(4.6 * S), LABEL_DIM);
    let entropia = st.params.fx_mode.value();
    let help = [
        "the LXR's own FX: one effect at a time, set by the kit (the display)",
        "every voice to delay, ring and reverb at once, then a master filter, drive and comp",
    ];
    for (i, name) in crate::entropia::FX_MODES.iter().enumerate() {
        let rect = if i == 0 {
            Rect::from_center_size(p(88.0, 153.0), Vec2::new(28.0 * S, 9.0 * S))
        } else {
            Rect::from_center_size(p(132.5, 153.0), Vec2::new(50.0 * S, 9.0 * S))
        };
        if lanes::small_button(ui, rect, name, entropia == (i == 1)).on_hover_text(help[i]).clicked() {
            setter.begin_set_parameter(&st.params.fx_mode);
            setter.set_parameter(&st.params.fx_mode, i == 1);
            setter.end_set_parameter(&st.params.fx_mode);
            st.entropia_open = i == 1;
        }
    }
}

fn small_label(painter: &egui::Painter, pos: Pos2, text: &str) {
    painter.text(pos, Align2::CENTER_CENTER, text, FontId::proportional(4.2 * S), LABEL);
}

/// A label printed dark in a white box, placed at `pos` by `align`.
fn boxed_label(painter: &egui::Painter, pos: Pos2, align: Align2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(4.0 * S), PANEL);
    let rect = align.anchor_size(pos, galley.size() + Vec2::new(2.4 * S, 0.4 * S));
    painter.rect_filled(rect, 1.5, Color32::from_rgb(228, 228, 230));
    painter.galley(rect.center() - galley.size() / 2.0, galley, PANEL);
}

fn group_box(painter: &egui::Painter, x0: f32, x1: f32) {
    painter.rect_stroke(
        Rect::from_min_max(p(x0, 168.5), p(x1, 193.0)),
        2.0,
        Stroke::new(1.0_f32, LABEL_DIM),
        StrokeKind::Inside,
    );
}

/// The shine on a button cap; a lit cap glows pale pink in the middle, like the unit's LEDs.
fn highlight(base: Color32, lit: bool) -> Color32 {
    if lit {
        Color32::from_rgb(255, 150, 190)
    } else {
        base.gamma_multiply(1.15)
    }
}

fn glow(painter: &egui::Painter, center: Pos2, radius: f32) {
    for i in 0..4 {
        let a = 30 - i * 7;
        painter.circle_filled(center, radius + (i as f32 + 1.0) * 2.0, Color32::from_rgba_unmultiplied(255, 66, 130, a as u8));
    }
}

fn round_button(ui: &mut Ui, center: Pos2, lit: bool, enabled: bool) -> Response {
    let r = 6.0 * S;
    let rect = Rect::from_center_size(center, Vec2::splat(r * 2.0));
    let resp = ui.allocate_rect(rect, if enabled { Sense::click() } else { Sense::hover() });
    let painter = ui.painter();
    if lit {
        glow(painter, center, r);
    }
    let base = if lit { LED_RED } else if enabled { BUTTON } else { BUTTON_DIM };
    painter.circle_filled(center, r, if lit { LED_RED.gamma_multiply(0.7) } else { Color32::from_rgb(12, 12, 13) });
    painter.circle_filled(center, r * 0.86, base);
    painter.circle_filled(center - Vec2::splat(r * 0.18), r * 0.5, highlight(base, lit));
    if resp.hovered() && enabled {
        painter.circle_stroke(center, r, Stroke::new(1.0_f32, LABEL));
    }
    resp
}

/// A step button as on the LXR-02: a round domed cap in a square bezel, the bezels of the beats
/// (steps 1, 5, 9, 13) white.
fn step_button(ui: &mut Ui, center: Pos2, lit: bool, enabled: bool, beat: bool) -> Response {
    let half = 12.5 * S;
    let rect = Rect::from_center_size(center, Vec2::splat(half * 2.0));
    let resp = ui.allocate_rect(rect, if enabled { Sense::click() } else { Sense::hover() });
    let painter = ui.painter();
    let r = half * 0.74;
    if lit {
        for i in 0..4 {
            painter.rect_filled(rect.expand((i + 1) as f32 * 2.0), 6.0, Color32::from_rgba_unmultiplied(255, 66, 130, (30 - i * 7) as u8));
        }
    }
    if beat {
        // the beats' bezels are white all over, the cap sits in a grey seat
        painter.rect_filled(rect, 5.0, Color32::from_rgb(226, 226, 230));
        painter.circle_filled(center, r * 1.08, Color32::from_rgb(150, 150, 156));
    } else {
        painter.rect_filled(rect, 5.0, Color32::from_rgb(12, 12, 13));
        painter.rect_stroke(rect.shrink(1.0), 4.5, Stroke::new(1.2_f32, Color32::from_rgb(58, 58, 63)), StrokeKind::Inside);
    }
    let base = if lit { LED_RED } else if enabled { BUTTON } else { BUTTON_DIM };
    painter.circle_filled(center + Vec2::new(0.0, 1.2 * S), r, Color32::from_rgb(4, 4, 5));
    painter.circle_filled(center, r, base.gamma_multiply(0.72));
    painter.circle_filled(center - Vec2::splat(r * 0.1), r * 0.84, base);
    painter.circle_filled(center - Vec2::splat(r * 0.3), r * 0.36, highlight(base, lit));
    if resp.hovered() && enabled {
        painter.circle_stroke(center, r, Stroke::new(1.0_f32, LABEL));
    }
    resp
}

// ---------------------------------------------------------------- LCD

/// The value as the LXR's LCD shows it (at most 4 characters, names cut to 3).
fn lcd_value(st: &State, slot: &Slot) -> String {
    if slot.param == sequencer::PAR_TRACK_LENGTH {
        return sequencer::track_length(st, sequencer::track(st)).to_string();
    }
    let is_mod_target = crate::param_table::PARAMS
        .iter()
        .any(|info| info.index == slot.param && matches!(info.display, Display::ModTarget));
    match st.params.by_index(slot.param) {
        Some(param) if is_mod_target => match MOD_TARGETS.get(param.value() as usize) {
            Some(t) if t.param != 0 => t.short.to_string(),
            _ => "off".to_string(),
        },
        Some(param) => {
            let s = param.to_string();
            let s = s.trim();
            if s.parse::<i32>().is_ok() {
                s.chars().take(4).collect()
            } else {
                s.chars().take(3).collect()
            }
        }
        None => "---".to_string(),
    }
}

fn draw_lcd(ui: &mut Ui, setter: &ParamSetter, st: &mut State) {
    let rect = Rect::from_min_max(p(40.0, 53.0), p(210.0, 90.0));
    let painter = ui.painter().clone();
    painter.rect_filled(rect.expand(3.0 * S), 3.0, Color32::from_rgb(8, 8, 9));
    painter.rect_filled(rect, 2.0, LCD_BG);

    let mut rows = [String::new(), String::new()];
    if st.page == Page::Pattern {
        rows = sequencer::lcd_rows(st);
    }
    if st.page == Page::Perf {
        rows = sequencer::perf_rows(st);
    }
    if st.page == Page::Save {
        rows = save::lcd_rows(st);
    }
    match st.page {
        Page::Step => rows = sequencer::step_rows(st),
        Page::Copy => rows = song::copy_rows(st),
        Page::Song => rows = song::song_rows(st),
        _ => {}
    }
    if st.page == Page::Load {
        let (start, end) = st.bank_range();
        let kit = &st.kits[st.kit_index.min(st.kits.len() - 1)];
        let loaded = if st.load_to_morph {
            st.params.morph_kit_name().as_deref() == Some(kit.name.as_str())
        } else {
            st.params.kit_name.lock().map(|n| *n == kit.name).unwrap_or(false)
        };
        let bank: String = kit.bank.chars().take(8).collect();
        rows[0] = format!("{bank:<8}{:>4}/{:<3}", st.kit_index - start + 1, end - start);
        let name: String = kit.name.chars().take(8).collect();
        let status = match (st.load_to_morph, loaded) {
            (false, true) => "loaded",
            (false, false) => "load?",
            (true, true) => "morph B",
            (true, false) => "morph?",
        };
        rows[1] = format!("{name:<8}{status:>8}");
    }
    for column in (0..4).filter(|_| matches!(st.page, Page::Voice(_) | Page::Fx)) {
        let (name, value) = match st.slot(column) {
            Some(slot) => {
                let name = if st.half * 4 + column == st.active {
                    slot.short.to_uppercase()
                } else {
                    slot.short.to_string()
                };
                (name, lcd_value(st, &slot))
            }
            None => (String::new(), String::new()),
        };
        rows[0].push_str(&format!("{name:<4}"));
        rows[1].push_str(&format!("{value:<4}"));
    }

    // 16x2 character cells of 5x8 dots
    let cell_w = rect.width() / 16.0;
    let cell_h = rect.height() / 2.0;
    let dot = Vec2::new(cell_w * 0.78 / 5.0, cell_h * 0.72 / 8.0);
    let font = FontId::monospace(cell_h * 0.78);
    for (row, text) in rows.iter().enumerate() {
        let chars: Vec<char> = text.chars().collect();
        for c in 0..16 {
            let cell = Rect::from_min_size(
                rect.min + Vec2::new(c as f32 * cell_w + cell_w * 0.11, row as f32 * cell_h + cell_h * 0.14),
                Vec2::new(dot.x * 5.0, dot.y * 8.0),
            );
            for dy in 0..8 {
                for dx in 0..5 {
                    let d = Rect::from_min_size(cell.min + Vec2::new(dx as f32 * dot.x, dy as f32 * dot.y), dot * 0.86);
                    painter.rect_filled(d, 0.0, LCD_OFF);
                }
            }
            if let Some(&ch) = chars.get(c) {
                if ch != ' ' {
                    painter.text(cell.center(), Align2::CENTER_CENTER, ch, font.clone(), LCD_ON);
                }
            }
            // grid lines between the dots give the text its dot-matrix look
            for dx in 1..5 {
                let x = cell.min.x + dx as f32 * dot.x - dot.x * 0.07;
                painter.line_segment([Pos2::new(x, cell.min.y), Pos2::new(x, cell.max.y)], Stroke::new(dot.x * 0.14, LCD_BG));
            }
            for dy in 1..8 {
                let y = cell.min.y + dy as f32 * dot.y - dot.y * 0.07;
                painter.line_segment([Pos2::new(cell.min.x, y), Pos2::new(cell.max.x, y)], Stroke::new(dot.y * 0.14, LCD_BG));
            }
        }
    }

    // on the voice and FX pages a click opens the preset browser; the browser pages use it to
    // confirm (load, play, save, ...)
    let resp = ui.allocate_rect(rect, Sense::click());
    let resp = if matches!(st.page, Page::Voice(_) | Page::Fx) { resp.on_hover_text("click: open the preset browser") } else { resp };
    if resp.clicked() && st.page == Page::Load {
        st.load_selected(setter);
    } else if resp.clicked() && st.page == Page::Pattern {
        sequencer::apply(st);
    } else if resp.clicked() && st.page == Page::Copy {
        let now = ui.input(|i| i.time);
        song::copy_click_display(st, now);
    } else if resp.clicked() && st.page == Page::Step {
        if let Some(pos) = resp.interact_pointer_pos() {
            st.step_col = (((pos.x - rect.min.x) / (rect.width() / 4.0)) as usize).min(3);
        }
    } else if resp.clicked() && st.page == Page::Save {
        let now = ui.input(|i| i.time);
        save::click_display(st, now);
    } else if resp.clicked() && st.page == Page::Perf {
        if let Some(pos) = resp.interact_pointer_pos() {
            st.perf_col = (((pos.x - rect.min.x) / (rect.width() / 4.0)) as usize).min(3);
        }
    } else if resp.clicked() {
        // on the voice and FX pages the display opens the preset browser
        st.browser = Some(browser::BrowserState::new(false));
    }
    if st.page == Page::Pattern {
        painter.text(p(125.0, 96.5), Align2::CENTER_CENTER, sequencer::caption(st), FontId::proportional(4.2 * S), LABEL_DIM);
    }
    if st.page == Page::Perf {
        painter.text(p(125.0, 96.5), Align2::CENTER_CENTER, sequencer::perf_caption(st), FontId::proportional(4.2 * S), LABEL_DIM);
    }
    if st.page == Page::Save {
        let now = ui.input(|i| i.time);
        save::name_field(ui, st, Rect::from_min_max(p(40.0, 93.6), p(80.0, 99.4)));
        painter.text(p(83.0, 96.5), Align2::LEFT_CENTER, save::caption(st, now), FontId::proportional(4.2 * S), LABEL_DIM);
    }
    let caption = match st.page {
        Page::Step => Some(sequencer::step_caption(st)),
        Page::Copy => Some(song::copy_caption(st, ui.input(|i| i.time))),
        Page::Song => Some(song::song_caption(st)),
        _ => None,
    };
    if let Some(caption) = caption {
        painter.text(p(125.0, 96.5), Align2::CENTER_CENTER, caption, FontId::proportional(4.0 * S), LABEL_DIM);
    }
    if st.page == Page::Load {
        painter.text(
            p(125.0, 96.5),
            Align2::CENTER_CENTER,
            &format!(
                "BANK · KIT · knob 3: load as {} · knob 4: morph {} to {} · click DATA or display",
                if st.load_to_morph { "MORPH (kit B)" } else { "KIT" },
                st.params.morph.value(),
                st.params.morph_kit_name().unwrap_or_else(|| "no kit B".to_string())
            ),
            FontId::proportional(4.6 * S),
            LABEL_DIM,
        );
    }
    // full name of the selected parameter under the display
    if let Some(slot) = st.slot(st.active % 4).filter(|_| st.active / 4 == st.half) {
        painter.text(
            p(125.0, 96.5),
            Align2::CENTER_CENTER,
            format!("{} · {}", slot.category, slot.long),
            FontId::proportional(4.6 * S),
            LABEL_DIM,
        );
    }
}

// ---------------------------------------------------------------- knobs

fn knob_face(painter: &egui::Painter, center: Pos2, radius: f32, normalized: Option<f32>, knurled: bool) {
    painter.circle_filled(center + Vec2::new(0.0, 1.5 * S), radius * 1.04, Color32::from_rgb(6, 6, 7));
    if knurled {
        // the DATA encoder: a black knob whose rim is cut into scallops, like the LXR-02's
        // the grip: a lighter rim with the scallops cut out of it
        painter.circle_filled(center, radius, Color32::from_rgb(70, 70, 75));
        painter.circle_filled(center + Vec2::new(0.0, 0.06 * radius), radius * 0.95, Color32::from_rgb(34, 34, 37));
        for i in 0..9 {
            let d = Vec2::angled(i as f32 / 9.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2);
            painter.circle_filled(center + d * radius * 1.05, radius * 0.19, PANEL);
        }
        painter.circle_stroke(center, radius * 0.8, Stroke::new(1.2_f32, Color32::from_rgb(62, 62, 67)));
        painter.circle_filled(center, radius * 0.78, Color32::from_rgb(16, 16, 18));
        painter.circle_filled(center - Vec2::splat(radius * 0.22), radius * 0.4, Color32::from_rgba_unmultiplied(255, 255, 255, 5));
        if let Some(n) = normalized {
            let d = Vec2::angled((-225.0 + 270.0 * n).to_radians());
            painter.line_segment([center + d * radius * 0.5, center + d * radius * 0.72], Stroke::new(2.4_f32, Color32::from_rgb(200, 200, 204)));
        }
        return;
    }
    painter.circle_filled(center, radius, Color32::from_rgb(22, 22, 24));
    painter.circle_stroke(center, radius * 0.96, Stroke::new(1.0_f32, Color32::from_rgb(46, 46, 50)));
    painter.circle_filled(center, radius * 0.74, Color32::from_rgb(176, 176, 180));
    painter.circle_filled(center - Vec2::splat(radius * 0.12), radius * 0.52, Color32::from_rgb(198, 198, 202));
    if let Some(n) = normalized {
        let a = (-225.0 + 270.0 * n).to_radians();
        let d = Vec2::angled(a);
        painter.line_segment([center + d * radius * 0.2, center + d * radius * 0.7], Stroke::new(2.0 * S * 0.6, Color32::from_rgb(40, 40, 44)));
    }
}

/// Destinations offered for a velocity/LFO destination parameter, like the LXR's menu: "off"
/// plus the targets of one voice (the voice itself for velocity, the LFO's "voi" setting for LFOs).
fn mod_target_choices(params: &LxrParams, index: u16) -> Option<Vec<i32>> {
    let voice = match index {
        PAR_VEL_DEST_1..=154 => (index - PAR_VEL_DEST_1 + 1) as u8,
        PAR_TARGET_LFO1..=172 => params.by_index(index - PAR_TARGET_LFO1 + PAR_VOICE_LFO1)?.value() as u8,
        _ => return None,
    };
    let mut choices = vec![0];
    choices.extend((0..MOD_TARGETS.len()).filter(|&i| MOD_TARGETS[i].voice == voice).map(|i| i as i32));
    Some(choices)
}

/// `steps` further along `choices` (or along the plain range without choices).
fn step_value(param: &IntParam, steps: i32, choices: Option<&[i32]>) -> i32 {
    match choices {
        Some(c) => {
            let pos = c.iter().position(|&v| v >= param.value()).unwrap_or(0) as i32;
            c[(pos + steps).clamp(0, c.len() as i32 - 1) as usize]
        }
        None => (param.value() + steps).clamp(param.preview_plain(0.0), param.preview_plain(1.0)),
    }
}

/// After an LFO's "voi" changed, point its destination at the same parameter of the new voice
/// (the LXR menu does the same).
fn follow_lfo_voice(setter: &ParamSetter, params: &LxrParams, voice_param: u16, old_voice: i32, new_voice: i32) {
    let Some(target) = params.by_index(voice_param - PAR_VOICE_LFO1 + PAR_TARGET_LFO1) else { return };
    let of_voice = |v: i32| -> Vec<usize> { (0..MOD_TARGETS.len()).filter(|&i| MOD_TARGETS[i].voice as i32 == v).collect() };
    let (old, new) = (of_voice(old_voice), of_voice(new_voice));
    if let Some(pos) = old.iter().position(|&i| i as i32 == target.value()) {
        if let Some(&t) = new.get(pos) {
            setter.begin_set_parameter(target);
            setter.set_parameter(target, t as i32);
            setter.end_set_parameter(target);
        }
    }
}

/// Scroll wheel / trackpad steps over a control: one step per 20 points, so momentum scrolling
/// doesn't race through the values.
fn scroll_steps(ui: &Ui, resp: &Response, accum: &mut f32) -> i32 {
    if !resp.hovered() {
        return 0;
    }
    *accum += ui.input(|i| i.raw_scroll_delta.y);
    let steps = (*accum / 20.0).trunc();
    *accum -= steps * 20.0;
    steps as i32
}

fn edit_param_drag(
    ui: &Ui,
    resp: &Response,
    setter: &ParamSetter,
    param: &IntParam,
    choices: Option<&[i32]>,
    accum: &mut f32,
    scroll_accum: &mut f32,
    px_per_step: f32,
) {
    if resp.drag_started() {
        setter.begin_set_parameter(param);
        *accum = 0.0;
    }
    if resp.dragged() {
        *accum -= resp.drag_delta().y;
        let steps = (*accum / px_per_step).trunc();
        if steps != 0.0 {
            *accum -= steps * px_per_step;
            setter.set_parameter(param, step_value(param, steps as i32, choices));
        }
    }
    if resp.drag_stopped() {
        setter.end_set_parameter(param);
    }
    if resp.double_clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, param.default_plain_value());
        setter.end_set_parameter(param);
    }
    let scroll = scroll_steps(ui, resp, scroll_accum);
    if scroll != 0 {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, step_value(param, scroll, choices));
        setter.end_set_parameter(param);
    }
}

/// Whole steps dragged since the last frame (up = positive), keeping the remainder in `accum`.
fn drag_steps(resp: &Response, accum: &mut f32, px_per_step: f32) -> i32 {
    if resp.drag_started() {
        *accum = 0.0;
    }
    if !resp.dragged() {
        return 0;
    }
    *accum -= resp.drag_delta().y;
    let steps = (*accum / px_per_step).trunc();
    *accum -= steps * px_per_step;
    steps as i32
}

/// Knob 1 picks the bank, knob 2 the kit in it, knob 3 (click) switches between loading the kit
/// and loading the morph kit B, knob 4 is the morph amount.
fn draw_load_knob(ui: &mut Ui, setter: &ParamSetter, st: &mut State, column: usize, center: Pos2, radius: f32, resp: &Response) {
    if column == 2 {
        knob_face(ui.painter(), center, radius, Some(if st.load_to_morph { 1.0 } else { 0.0 }), false);
        if resp.clicked() || drag_steps(resp, &mut st.drag_accum, 20.0) != 0 {
            st.load_to_morph = !st.load_to_morph;
        }
        return;
    }
    if column == 3 {
        let params = st.params.clone();
        knob_face(ui.painter(), center, radius, Some(params.morph.unmodulated_normalized_value()), false);
        edit_param_drag(ui, resp, setter, &params.morph, None, &mut st.drag_accum, &mut st.scroll_accum, 1.0);
        return;
    }
    let starts = st.bank_starts();
    let bank = starts.iter().rposition(|&s| s <= st.kit_index).unwrap_or(0);
    let (start, end) = st.bank_range();
    let turn = match column {
        0 => Some(bank as f32 / (starts.len().max(2) - 1) as f32),
        1 => Some((st.kit_index - start) as f32 / (end - start).max(2).saturating_sub(1) as f32),
        _ => None,
    };
    knob_face(ui.painter(), center, radius, turn, false);
    let scroll = scroll_steps(ui, resp, &mut st.scroll_accum);
    let steps = drag_steps(resp, &mut st.drag_accum, if column == 0 { 30.0 } else { 12.0 }) + scroll;
    if steps == 0 {
        return;
    }
    match column {
        0 => {
            let b = (bank as i32 + steps).clamp(0, starts.len() as i32 - 1) as usize;
            st.kit_index = starts[b];
        }
        1 => st.kit_index = (st.kit_index as i32 + steps).clamp(start as i32, end as i32 - 1) as usize,
        _ => {}
    }
}

fn draw_knob(ui: &mut Ui, setter: &ParamSetter, st: &mut State, column: usize) {
    let center = p(KNOB_X[column], 127.0);
    let radius = 13.0 * S;
    let resp = ui.allocate_rect(Rect::from_center_size(center, Vec2::splat(radius * 2.0)), Sense::click_and_drag());
    if st.page == Page::Load {
        draw_load_knob(ui, setter, st, column, center, radius, &resp);
        return;
    }
    if st.page == Page::Pattern {
        sequencer::knob(ui, st, column, center, radius, &resp);
        return;
    }
    if st.page == Page::Perf {
        sequencer::perf_knob(ui, st, column, center, radius, &resp);
        return;
    }
    if st.page == Page::Save {
        save::knob(ui, st, column, center, radius, &resp);
        return;
    }
    match st.page {
        Page::Step => return sequencer::step_knob(ui, st, column, center, radius, &resp),
        Page::Copy => return song::copy_knob(ui, st, column, center, radius, &resp),
        Page::Song => return song::song_knob(ui, st, column, center, radius, &resp),
        _ => {}
    }
    let index = st.slot(column).map(|s| s.param);
    if index == Some(sequencer::PAR_TRACK_LENGTH) {
        // the MIX page's "len" is the track length in the shown pattern
        let track = sequencer::track(st);
        let len = sequencer::track_length(st, track);
        knob_face(ui.painter(), center, radius, Some((len as f32 - 1.0) / 63.0), false);
        if resp.drag_started() || resp.clicked() {
            st.active = st.half * 4 + column;
        }
        let delta = drag_steps(&resp, &mut st.drag_accum, 4.0) + scroll_steps(ui, &resp, &mut st.scroll_accum);
        sequencer::change_track_length(st, track, delta);
        if turning(ui, &resp) {
            st.popup = Some((center - Vec2::new(0.0, radius + 6.0), format!("track length: {}", sequencer::track_length(st, track))));
        }
        return;
    }
    let params = st.params.clone();
    let param = index.and_then(|i| params.by_index(i));
    knob_face(ui.painter(), center, radius, param.map(|p| p.unmodulated_normalized_value()), false);
    if let (Some(param), Some(index)) = (param, index) {
        let resp = match st.slot(column) {
            Some(slot) => resp.on_hover_text(format!("{} · {}\ndrag or scroll · double-click resets", slot.category, slot.long)),
            None => resp,
        };
        if turning(ui, &resp) {
            st.popup = Some((center - Vec2::new(0.0, radius + 6.0), format!("{}: {}", param.name(), param)));
        }
        if resp.drag_started() || resp.clicked() {
            st.active = st.half * 4 + column;
        }
        let choices = mod_target_choices(&params, index);
        // whole range in about 200 px, but never more than 30 px per step
        let steps = choices.as_ref().map_or((param.preview_plain(1.0) - param.preview_plain(0.0)).max(1), |c| c.len() as i32) as f32;
        let before = param.value();
        edit_param_drag(ui, &resp, setter, param, choices.as_deref(), &mut st.drag_accum, &mut st.scroll_accum, (200.0 / steps).clamp(1.0, 30.0));
        if (PAR_VOICE_LFO1..PAR_VOICE_LFO1 + 6).contains(&index) && param.value() != before {
            follow_lfo_voice(setter, &params, index, before, param.value());
        }
    }
}

/// the DATA encoder's centre, with room between it and the LCD like on the unit
const DATA_X: f32 = 257.0;

fn draw_encoder(ui: &mut Ui, setter: &ParamSetter, st: &mut State) {
    let center = p(DATA_X, 71.5);
    let radius = 21.0 * S;
    let painter = ui.painter().clone();
    for i in 0..=48 {
        let a = (-225.0 + 270.0 * i as f32 / 48.0).to_radians();
        let d = Vec2::angled(a);
        let long = i % 4 == 0;
        let outer = if long { 1.36 } else { 1.3 };
        painter.line_segment([center + d * radius * 1.24, center + d * radius * outer], Stroke::new(if long { 1.4_f32 } else { 0.8 }, LABEL));
    }
    painter.text(p(DATA_X, 100.0), Align2::CENTER_CENTER, "DATA", FontId::proportional(4.4 * S), LABEL);
    let resp = ui.allocate_rect(Rect::from_center_size(center, Vec2::splat(radius * 2.0)), Sense::click_and_drag());
    if st.page == Page::Pattern {
        sequencer::encoder(ui, st, center, radius, &resp);
        return;
    }
    if st.page == Page::Perf {
        sequencer::perf_encoder(ui, st, center, radius, &resp);
        return;
    }
    if st.page == Page::Save {
        save::encoder(ui, st, center, radius, &resp);
        return;
    }
    match st.page {
        Page::Step => return sequencer::step_encoder(ui, st, center, radius, &resp),
        Page::Copy => return song::copy_encoder(ui, st, center, radius, &resp),
        Page::Song => return song::song_encoder(ui, st, center, radius, &resp),
        _ => {}
    }
    if st.page == Page::Load {
        // DATA scrolls through all kits; a click loads the selected one
        let n = st.kits.len().max(1);
        knob_face(&painter, center, radius, Some(st.kit_index as f32 / (n.max(2) - 1) as f32), true);
        let scroll = scroll_steps(ui, &resp, &mut st.scroll_accum);
        let steps = drag_steps(&resp, &mut st.drag_accum, 8.0) + scroll;
        st.kit_index = (st.kit_index as i32 + steps).clamp(0, n as i32 - 1) as usize;
        if resp.clicked() {
            st.load_selected(setter);
        }
        return;
    }
    let index = st.sub_page()[st.active].map(|s| s.param);
    if index == Some(sequencer::PAR_TRACK_LENGTH) {
        let track = sequencer::track(st);
        knob_face(&painter, center, radius, Some((sequencer::track_length(st, track) as f32 - 1.0) / 63.0), true);
        let delta = drag_steps(&resp, &mut st.drag_accum, 8.0) + scroll_steps(ui, &resp, &mut st.scroll_accum);
        sequencer::change_track_length(st, track, delta);
        return;
    }
    let params = st.params.clone();
    let param = index.and_then(|i| params.by_index(i));
    let turn = param.map(|p| p.unmodulated_normalized_value());
    knob_face(&painter, center, radius, turn, true);
    if let (Some(param), Some(index)) = (param, index) {
        let resp = resp.on_hover_text("DATA: the selected parameter (capitals on the display) in single steps");
        if turning(ui, &resp) {
            st.popup = Some((center - Vec2::new(0.0, radius + 6.0), format!("{}: {}", param.name(), param)));
        }
        let choices = mod_target_choices(&params, index);
        let before = param.value();
        edit_param_drag(ui, &resp, setter, param, choices.as_deref(), &mut st.drag_accum, &mut st.scroll_accum, 8.0);
        if (PAR_VOICE_LFO1..PAR_VOICE_LFO1 + 6).contains(&index) && param.value() != before {
            follow_lfo_voice(setter, &params, index, before, param.value());
        }
    }
}

// ---------------------------------------------------------------- faders

fn draw_faders(ui: &mut Ui, setter: &ParamSetter, st: &mut State) {
    let painter = ui.painter().clone();
    let (top, bottom) = (30.0 * S, 116.0 * S);
    for (i, label) in FADER_LABELS.iter().enumerate() {
        let x = COLUMN_X[i] * S;
        painter.text(p(COLUMN_X[i], 22.0), Align2::CENTER_CENTER, label, FontId::proportional(4.0 * S), LABEL);
        let param: &FloatParam = if i < 6 { &st.params.faders[i] } else { &st.params.volume };
        let slot = Rect::from_min_max(Pos2::new(x - 2.0 * S, top), Pos2::new(x + 2.0 * S, bottom));
        painter.rect_filled(slot.expand2(Vec2::new(7.0 * S, 3.0 * S)), 2.0, Color32::from_rgb(14, 14, 15));
        painter.rect_filled(slot, 1.0, Color32::from_rgb(4, 4, 4));
        for t in 0..=10 {
            let y = bottom - (bottom - top) * t as f32 / 10.0;
            painter.line_segment([Pos2::new(x - 6.0 * S, y), Pos2::new(x - 4.0 * S, y)], Stroke::new(1.0_f32, LABEL_DIM));
        }

        let travel = (bottom - top) - 10.0 * S;
        let n = param.unmodulated_normalized_value();
        let cap_y = bottom - 5.0 * S - travel * n;
        let cap = Rect::from_center_size(Pos2::new(x, cap_y), Vec2::new(11.0 * S, 9.0 * S));
        painter.rect_filled(cap.translate(Vec2::new(0.0, 1.0 * S)), 1.5, Color32::from_rgb(5, 5, 5));
        painter.rect_filled(cap, 1.5, Color32::from_rgb(40, 40, 43));
        painter.line_segment(
            [Pos2::new(cap.min.x + 1.0, cap_y), Pos2::new(cap.max.x - 1.0, cap_y)],
            Stroke::new(1.6 * S * 0.7, Color32::from_rgb(235, 235, 235)),
        );

        let hit = Rect::from_min_max(Pos2::new(x - 8.0 * S, top), Pos2::new(x + 8.0 * S, bottom));
        let resp = ui.allocate_rect(hit, Sense::click_and_drag());
        if resp.drag_started() {
            setter.begin_set_parameter(param);
        }
        if resp.dragged() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let v = ((bottom - 5.0 * S - pos.y) / travel).clamp(0.0, 1.0);
                setter.set_parameter_normalized(param, v);
            }
        }
        if resp.drag_stopped() {
            setter.end_set_parameter(param);
        }
        if resp.double_clicked() {
            setter.begin_set_parameter(param);
            setter.set_parameter(param, param.default_plain_value());
            setter.end_set_parameter(param);
        }
        if resp.dragged() {
            st.popup = Some((Pos2::new(x, cap_y - 9.0 * S), format!("{label}: {param}")));
        }
        resp.on_hover_text(format!("{label} level: {param}\ndrag · double-click resets"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_choices_follow_the_voice() {
        let params = LxrParams::new();
        // velocity destination of Drum 2: "off" plus Drum 2's targets only
        let vel = mod_target_choices(&params, PAR_VEL_DEST_1 + 1).unwrap();
        assert_eq!(vel[0], 0);
        assert!(vel[1..].iter().all(|&i| MOD_TARGETS[i as usize].voice == 2));
        assert_eq!(vel.len() - 1, MOD_TARGETS.iter().filter(|t| t.voice == 2).count());

        // stepping walks the list and stops at its ends
        let p = IntParam::new("t", 0, IntRange::Linear { min: 0, max: MOD_TARGETS.len() as i32 - 1 });
        assert_eq!(step_value(&p, 1, Some(&vel)), vel[1]);
        assert_eq!(step_value(&p, -5, Some(&vel)), 0);
        assert_eq!(step_value(&p, 10_000, Some(&vel)), *vel.last().unwrap());
        assert_eq!(step_value(&p, 3, None), 3);
    }
}
