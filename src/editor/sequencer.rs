//! Sequencer controls on the panel: the 16 step buttons edit the selected voice's track in the
//! shown bar, PATTERN opens the pattern/project browser, and the kit of a pattern is loaded when
//! the sequencer switches to it.

use super::*;
use crate::seq::{self, NUM_PATTERNS};

pub(super) const STEP_PATTERN: usize = 15;

/// Track edited by the step buttons: the selected voice (CL HH and OP HH are tracks 5 and 6).
pub(super) fn track(st: &State) -> usize {
    match st.page {
        Page::Voice(v) => v,
        _ => st.last_track,
    }
}

/// Pattern the step buttons show: the one selected to play next (the playing one, once it runs).
fn shown_pattern(st: &State) -> usize {
    st.params.seq.pending.load(Ordering::Acquire) as usize % NUM_PATTERNS
}

/// The step buttons in sequencer mode.
pub(super) fn draw_steps(ui: &mut Ui, st: &mut State) {
    let painter = ui.painter().clone();
    let params = st.params.clone();
    let seq_state = &params.seq;
    let track = track(st);
    let pattern = shown_pattern(st);
    let running = seq_state.running.load(Ordering::Acquire);
    let playing_here = running && seq_state.active.load(Ordering::Acquire) as usize == pattern;
    let playhead = seq_state.step[track].load(Ordering::Acquire) as usize;
    let track_len = track_length(st, track);
    let steps: Vec<bool> = match seq_state.patterns.lock() {
        Ok(p) => (0..16).map(|i| seq::step_on(&p[pattern], track, st.bar * 16 + i)).collect(),
        Err(_) => vec![false; 16],
    };
    for (i, &on) in steps.iter().enumerate() {
        let step = st.bar * 16 + i;
        let center = p(25.0 + i as f32 * 31.4, 209.0);
        let resp = step_button(ui, center, on, true, i % 4 == 0)
            .on_hover_text(format!("step {}: click on/off · right-click or ⌘-click: edit details", step + 1));
        if playing_here && playhead == step {
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::splat(25.0 * S)).expand(2.0),
                6.0,
                Stroke::new(2.5_f32, Color32::WHITE),
                StrokeKind::Outside,
            );
        }
        if st.page == Page::Step && st.sel_step == Some(step) {
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::splat(25.0 * S)).expand(5.0),
                7.0,
                Stroke::new(2.0_f32, Color32::from_rgb(255, 200, 60)),
                StrokeKind::Outside,
            );
        }
        // right-click or cmd-click selects the step for editing; a plain click turns it on/off
        let select = resp.secondary_clicked() || (resp.clicked() && ui.input(|i| i.modifiers.command));
        if select {
            st.sel_step = Some(step);
            if st.page != Page::Step {
                st.toggle_page(Page::Step);
            }
        } else if resp.hovered() && ui.input(|inp| inp.pointer.primary_pressed() && !inp.modifiers.command) {
            if let Ok(mut p) = seq_state.patterns.lock() {
                seq::toggle_step(&mut p[pattern], track, step);
            }
            seq_state.mark_dirty(pattern);
        }
        let label = if step < track_len as usize { format!("{}", step + 1) } else { "·".to_string() };
        small_label(&painter, center + Vec2::new(0.0, 19.0 * S * 0.5 + 4.5 * S), &label);
    }
}

/// When the sequencer moves to another pattern, load the kit that pattern was made with.
pub(super) fn follow_kit(setter: &ParamSetter, st: &mut State) {
    let active = st.params.seq.active.load(Ordering::Acquire);
    let changed = st.last_active.is_some_and(|last| last != active);
    st.last_active = Some(active);
    if !changed || !st.params.seq.kit_follows.load(Ordering::Acquire) {
        return;
    }
    let kit_nr = match st.params.seq.patterns.lock() {
        Ok(p) => seq::kit_number(&p[active as usize % NUM_PATTERNS]),
        Err(_) => return,
    };
    let project_kit = st.params.seq.project_kits.lock().ok().and_then(|k| k.get(kit_nr).cloned().flatten());
    if let Some((name, data)) = project_kit {
        let bank = st.params.seq.project_name().unwrap_or_default();
        kits::load(setter, &st.params, &Kit { bank, name, data });
    }
}

// ---------------------------------------------------------------- pattern page

pub(super) fn lcd_rows(st: &State) -> [String; 2] {
    let Some(project) = st.projects.get(st.proj_sel) else { return [String::new(), String::new()] };
    let seq_state = &st.params.seq;
    let loaded = seq_state.project_name().as_deref() == Some(project.name.as_str());
    // a loaded project shows the session's patterns and kits, which may have been edited
    let (kit, empty) = if loaded {
        let (kit_nr, empty) = match seq_state.patterns.lock() {
            Ok(p) => (seq::kit_number(&p[st.pat_sel]), seq::is_empty(&p[st.pat_sel])),
            Err(_) => (0, true),
        };
        let kit = seq_state.project_kits.lock().ok().and_then(|k| k.get(kit_nr).cloned().flatten()).map(|k| k.0);
        (kit.unwrap_or_default(), empty)
    } else {
        let pat = project.pattern(st.pat_sel);
        (project.kit(seq::kit_number(&pat)).map(|k| k.0.clone()).unwrap_or_default(), seq::is_empty(&pat))
    };
    let status = if loaded && seq_state.active.load(Ordering::Acquire) as usize == st.pat_sel {
        "play"
    } else if loaded && seq_state.pending.load(Ordering::Acquire) as usize == st.pat_sel {
        "next"
    } else if empty {
        "empty"
    } else {
        "load?"
    };
    let name: String = project.name.chars().take(9).collect();
    let kit = kit.as_str();
    let kit: String = if kit.is_empty() { "--".to_string() } else { kit.chars().take(8).collect() };
    [format!("{name:<10}P{:02}{}", st.pat_sel + 1, if loaded { "  *" } else { "   " }), format!("{kit:<9}{status:>7}")]
}

pub(super) fn caption(st: &State) -> String {
    let follows = if st.params.seq.kit_follows.load(Ordering::Acquire) { "on" } else { "off" };
    format!("PROJECT · PATTERN · kit follows: {follows} (knob 3) · click DATA or display to play")
}

/// Plays the selected pattern, loading its project first if another one is loaded.
pub(super) fn apply(st: &mut State) {
    let seq_state = &st.params.seq;
    let Some(project) = st.projects.get(st.proj_sel) else { return };
    if seq_state.project_name().as_deref() != Some(project.name.as_str()) {
        seq_state.load_project(project);
    }
    seq_state.request_pattern(st.pat_sel);
    // already playing it: there will be no switch, so load its kit now
    if seq_state.active.load(Ordering::Acquire) as usize == st.pat_sel {
        st.last_active = Some(u8::MAX);
    }
}

/// Knob 1 picks the project, knob 2 the pattern, a click on knob 3 toggles "kit follows pattern".
pub(super) fn knob(ui: &mut Ui, st: &mut State, column: usize, center: Pos2, radius: f32, resp: &Response) {
    let turn = match column {
        0 => Some(st.proj_sel as f32 / (st.projects.len().max(2) - 1) as f32),
        1 => Some(st.pat_sel as f32 / (NUM_PATTERNS - 1) as f32),
        2 => Some(if st.params.seq.kit_follows.load(Ordering::Acquire) { 1.0 } else { 0.0 }),
        _ => None,
    };
    knob_face(ui.painter(), center, radius, turn, false);
    if column == 2 && resp.clicked() {
        st.params.seq.kit_follows.fetch_xor(true, Ordering::AcqRel);
    }
    let scroll = scroll_steps(ui, resp, &mut st.scroll_accum);
    let steps = drag_steps(resp, &mut st.drag_accum, if column == 0 { 30.0 } else { 6.0 }) + scroll;
    match column {
        0 => st.proj_sel = (st.proj_sel as i32 + steps).clamp(0, st.projects.len() as i32 - 1) as usize,
        1 => st.pat_sel = (st.pat_sel as i32 + steps).clamp(0, NUM_PATTERNS as i32 - 1) as usize,
        _ => {}
    }
}

/// DATA on the pattern page: scrolls patterns, a click plays the selected one.
pub(super) fn encoder(ui: &mut Ui, st: &mut State, center: Pos2, radius: f32, resp: &Response) {
    knob_face(ui.painter(), center, radius, Some(st.pat_sel as f32 / (NUM_PATTERNS - 1) as f32), true);
    let scroll = scroll_steps(ui, resp, &mut st.scroll_accum);
    let steps = drag_steps(resp, &mut st.drag_accum, 8.0) + scroll;
    st.pat_sel = (st.pat_sel as i32 + steps).clamp(0, NUM_PATTERNS as i32 - 1) as usize;
    if resp.clicked() {
        apply(st);
    }
}

// ---------------------------------------------------------------- PERF page: euclid + shuffle

const PERF_NAMES: [&str; 4] = ["len", "stp", "rot", "shf"];

fn perf_values(st: &State) -> [u8; 4] {
    let track = track(st);
    let e = st.params.seq.euclid.lock().map(|e| e[track]).unwrap_or_default();
    let shuffle = st.params.seq.patterns.lock().map(|p| seq::shuffle(&p[shown_pattern(st)])).unwrap_or(0);
    [e.length, e.steps, e.rotation, shuffle]
}

pub(super) fn perf_rows(st: &State) -> [String; 2] {
    let values = perf_values(st);
    let mut rows = [String::new(), String::new()];
    for (i, name) in PERF_NAMES.iter().enumerate() {
        let name = if i == st.perf_col { name.to_uppercase() } else { name.to_string() };
        rows[0].push_str(&format!("{name:<4}"));
        rows[1].push_str(&format!("{:<4}", values[i]));
    }
    rows
}

pub(super) fn perf_caption(st: &State) -> String {
    format!(
        "EUCLID of {} (voice buttons pick the track) · SHUFFLE of pattern P{:02}",
        VOICE_LABELS[track(st)],
        shown_pattern(st) + 1
    )
}

/// Changes one PERF value by `delta`. Euclid changes regenerate the track's steps, like the LXR's
/// euclid page; shuffle is stored in the pattern.
fn perf_edit(st: &State, column: usize, delta: i32) {
    if delta == 0 {
        return;
    }
    let seq_state = &st.params.seq;
    let pattern = shown_pattern(st);
    if column == 3 {
        if let Ok(mut p) = seq_state.patterns.lock() {
            let v = (seq::shuffle(&p[pattern]) as i32 + delta).clamp(0, 127) as u8;
            seq::set_shuffle(&mut p[pattern], v);
        }
        seq_state.mark_dirty(pattern);
        return;
    }
    let track = track(st);
    let e = {
        let Ok(mut euclid) = seq_state.euclid.lock() else { return };
        let e = &mut euclid[track];
        match column {
            0 => e.length = (e.length as i32 + delta).clamp(1, 16) as u8,
            1 => e.steps = (e.steps as i32 + delta).clamp(1, 16) as u8,
            _ => e.rotation = (e.rotation as i32 + delta).clamp(0, 15) as u8,
        }
        // euklid_setSteps() / euklid_setRotation() limits
        e.steps = e.steps.min(e.length);
        e.rotation = e.rotation.min(e.length.min(15));
        *e
    };
    if let Ok(mut p) = seq_state.patterns.lock() {
        seq::apply_euclid(&mut p[pattern], track, e);
    }
    seq_state.mark_dirty(pattern);
}

fn perf_normalized(column: usize, v: u8) -> f32 {
    match column {
        0 | 1 => (v as f32 - 1.0) / 15.0,
        2 => v as f32 / 15.0,
        _ => v as f32 / 127.0,
    }
}

pub(super) fn perf_knob(ui: &mut Ui, st: &mut State, column: usize, center: Pos2, radius: f32, resp: &Response) {
    let values = perf_values(st);
    knob_face(ui.painter(), center, radius, Some(perf_normalized(column, values[column])), false);
    if resp.drag_started() || resp.clicked() {
        st.perf_col = column;
    }
    let scroll = scroll_steps(ui, resp, &mut st.scroll_accum);
    let px = if column == 3 { 2.0 } else { 12.0 };
    let delta = drag_steps(resp, &mut st.drag_accum, px) + scroll;
    perf_edit(st, column, delta);
}

/// DATA on the PERF page edits the selected value in single steps.
pub(super) fn perf_encoder(ui: &mut Ui, st: &mut State, center: Pos2, radius: f32, resp: &Response) {
    let values = perf_values(st);
    knob_face(ui.painter(), center, radius, Some(perf_normalized(st.perf_col, values[st.perf_col])), true);
    let scroll = scroll_steps(ui, resp, &mut st.scroll_accum);
    let delta = drag_steps(resp, &mut st.drag_accum, 8.0) + scroll;
    perf_edit(st, st.perf_col, delta);
}

// ---------------------------------------------------------------- step edit page

/// The step edit page's three views, like the LXR's: velocity/note/probability, flam/shift, and the
/// two automations (parameter locks).
const STEP_VIEWS: [[&str; 4]; 3] = [["vel", "nte", "prb", ""], ["flm", "shf", "", ""], ["dst", "val", "dst", "val"]];

const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Note name as Live shows it (60 = C3, the voice's programmed pitch).
pub(super) fn note_name(n: u8) -> String {
    format!("{}{}", NOTE_NAMES[n as usize % 12], n as i32 / 12 - 2)
}

fn selected(st: &State) -> Option<(usize, usize, seq::StepData)> {
    let step = st.sel_step?;
    let track = track(st);
    let data = st.params.seq.patterns.lock().ok().map(|p| seq::step(&p[shown_pattern(st)], track, step))?;
    Some((track, step, data))
}

fn dest_short(dest: Option<u16>) -> String {
    match dest.and_then(|p| MOD_TARGETS.iter().find(|t| t.param == p && t.param != 0)) {
        Some(t) => t.short.to_string(),
        None => "off".to_string(),
    }
}

fn step_value_text(d: &seq::StepData, view: usize, col: usize) -> String {
    match (view, col) {
        (0, 0) => d.velocity.to_string(),
        (0, 1) => note_name(d.note),
        (0, 2) => d.prob.to_string(),
        (1, 0) => seq::FLAM_NAMES[d.flam as usize].to_string(),
        (1, 1) => format!("{:+}", d.shift),
        (2, 0) => dest_short(d.dest[0]),
        (2, 1) => d.value[0].to_string(),
        (2, 2) => dest_short(d.dest[1]),
        (2, 3) => d.value[1].to_string(),
        _ => String::new(),
    }
}

pub(super) fn step_rows(st: &State) -> [String; 2] {
    let Some((_, _, d)) = selected(st) else { return ["no step".to_string(), String::new()] };
    let mut rows = [String::new(), String::new()];
    for (col, name) in STEP_VIEWS[st.step_view].iter().enumerate() {
        let name = if col == st.step_col && !name.is_empty() { name.to_uppercase() } else { name.to_string() };
        rows[0].push_str(&format!("{name:<4}"));
        rows[1].push_str(&format!("{:<4}", step_value_text(&d, st.step_view, col)));
    }
    rows
}

pub(super) fn step_caption(st: &State) -> String {
    match selected(st) {
        Some((track, step, _)) => format!(
            "STEP {} of {} · ◀ ▶ switch view {}/3 · knobs edit, double-click resets",
            step + 1,
            VOICE_LABELS[track],
            st.step_view + 1
        ),
        None => "right-click (or ⌘-click) a step to edit it".to_string(),
    }
}

/// Automation destinations offered for a track: "off" and the targets of its voice.
pub(super) fn dest_choices(track: usize) -> Vec<Option<u16>> {
    let voice = (track.min(5) + 1) as u8;
    std::iter::once(None)
        .chain(MOD_TARGETS.iter().filter(|t| t.voice == voice && t.param != 0).map(|t| Some(t.param)))
        .collect()
}

fn edit_step(st: &State, col: usize, delta: i32, reset: bool) {
    let Some((track, step, mut d)) = selected(st) else { return };
    if delta == 0 && !reset {
        return;
    }
    let step_by = |v: u8, lo: i32, hi: i32| (v as i32 + delta).clamp(lo, hi) as u8;
    match (st.step_view, col) {
        (0, 0) => d.velocity = if reset { 127 } else { step_by(d.velocity, 0, 127) },
        (0, 1) => d.note = if reset { 60 } else { step_by(d.note, 0, 127) },
        (0, 2) => d.prob = if reset { 100 } else { step_by(d.prob, 0, 100) },
        (1, 0) => d.flam = if reset { 0 } else { step_by(d.flam, 0, 5) },
        (1, 1) => d.shift = if reset { 0 } else { (d.shift as i32 + delta).clamp(-7, 7) as i8 },
        (2, 0) | (2, 2) => {
            let slot = col / 2;
            let choices = dest_choices(track);
            let pos = choices.iter().position(|c| *c == d.dest[slot]).unwrap_or(0) as i32;
            d.dest[slot] = if reset { None } else { choices[(pos + delta).clamp(0, choices.len() as i32 - 1) as usize] };
        }
        (2, 1) | (2, 3) => {
            let slot = col / 2;
            d.value[slot] = if reset { 0 } else { step_by(d.value[slot], 0, 127) };
        }
        _ => return,
    }
    let pattern = shown_pattern(st);
    if let Ok(mut p) = st.params.seq.patterns.lock() {
        seq::set_step(&mut p[pattern], track, step, d);
    }
    st.params.seq.mark_dirty(pattern);
}

fn step_normalized(d: &seq::StepData, view: usize, col: usize) -> Option<f32> {
    Some(match (view, col) {
        (0, 0) => d.velocity as f32 / 127.0,
        (0, 1) => d.note as f32 / 127.0,
        (0, 2) => d.prob as f32 / 100.0,
        (1, 0) => d.flam as f32 / 5.0,
        (1, 1) => (d.shift as f32 + 7.0) / 14.0,
        (2, 0) => d.dest[0].map_or(0.0, |_| 1.0),
        (2, 1) => d.value[0] as f32 / 127.0,
        (2, 2) => d.dest[1].map_or(0.0, |_| 1.0),
        (2, 3) => d.value[1] as f32 / 127.0,
        _ => return None,
    })
}

pub(super) fn step_knob(ui: &mut Ui, st: &mut State, column: usize, center: Pos2, radius: f32, resp: &Response) {
    let turn = selected(st).and_then(|(_, _, d)| step_normalized(&d, st.step_view, column));
    knob_face(ui.painter(), center, radius, turn, false);
    if turn.is_none() {
        return;
    }
    if resp.drag_started() || resp.clicked() {
        st.step_col = column;
    }
    let scroll = scroll_steps(ui, resp, &mut st.scroll_accum);
    let px = if st.step_view == 0 && column != 1 || st.step_view == 2 && column % 2 == 1 { 2.0 } else { 10.0 };
    let delta = drag_steps(resp, &mut st.drag_accum, px) + scroll;
    edit_step(st, column, delta, resp.double_clicked());
}

pub(super) fn step_encoder(ui: &mut Ui, st: &mut State, center: Pos2, radius: f32, resp: &Response) {
    let turn = selected(st).and_then(|(_, _, d)| step_normalized(&d, st.step_view, st.step_col));
    knob_face(ui.painter(), center, radius, turn, true);
    let scroll = scroll_steps(ui, resp, &mut st.scroll_accum);
    let delta = drag_steps(resp, &mut st.drag_accum, 8.0) + scroll;
    edit_step(st, st.step_col, delta, false);
}

/// ◀ ▶ on the step page.
pub(super) fn step_view(st: &mut State, delta: i32) {
    st.step_view = (st.step_view as i32 + delta).rem_euclid(STEP_VIEWS.len() as i32) as usize;
    if STEP_VIEWS[st.step_view][st.step_col].is_empty() {
        st.step_col = 0;
    }
}

// ---------------------------------------------------------------- track length (MIX "len")

/// PAR_TRACK_LENGTH: shown on the voice MIX page; the plugin keeps it in the pattern.
pub(super) const PAR_TRACK_LENGTH: u16 = 264;

pub(super) fn track_length(st: &State, track: usize) -> u8 {
    st.params.seq.patterns.lock().map(|p| seq::track_length(&p[shown_pattern(st)], track)).unwrap_or(16)
}

pub(super) fn change_track_length(st: &State, track: usize, delta: i32) {
    if delta == 0 {
        return;
    }
    let pattern = shown_pattern(st);
    if let Ok(mut p) = st.params.seq.patterns.lock() {
        let len = (seq::track_length(&p[pattern], track) as i32 + delta).clamp(1, 64) as u8;
        seq::set_track_length(&mut p[pattern], track, len);
    }
    st.params.seq.mark_dirty(pattern);
}
