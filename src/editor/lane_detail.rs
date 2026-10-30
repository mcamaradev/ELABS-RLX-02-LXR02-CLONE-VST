//! The detail strip under the LANES grid: one setting of every step of the selected track as bars,
//! like a DAW's velocity lane. Drag to draw, right-click a step to reset it. Everything the LXR's
//! sequencer keeps per step: velocity, probability, note, ratchet (the firmware's flam: the step
//! repeated x2-x8), shift (micro timing) and the two parameter locks.

use super::*;
use crate::seq::{self, StepData, FLAM_NAMES, NUM_PATTERNS};
use crate::ui_pages::MOD_TARGETS;

#[derive(Clone, Copy, PartialEq)]
enum Detail {
    Velocity,
    Prob,
    Note,
    Ratchet,
    Shift,
    Lock(usize),
}

const DETAILS: [(&str, Detail); 7] = [
    ("VELOCITY", Detail::Velocity),
    ("PROB", Detail::Prob),
    ("NOTE", Detail::Note),
    ("RATCHET", Detail::Ratchet),
    ("SHIFT", Detail::Shift),
    ("LOCK 1", Detail::Lock(0)),
    ("LOCK 2", Detail::Lock(1)),
];

const HELP: [&str; 7] = [
    "how hard each step hits",
    "chance that each step plays",
    "pitch of each step (C3 = the voice's own pitch)",
    "repeats inside the step (the LXR's flam): off, x2, x3, x4, x6, x8",
    "nudges each step earlier or later (micro timing)",
    "parameter lock: a sound setting only on that step (pick the parameter with ◀ ▶)",
    "second parameter lock",
];

/// lowest and highest value, the cleared value, and the centre line of two-sided settings
fn range(detail: Detail) -> (i32, i32, i32, Option<i32>) {
    match detail {
        Detail::Velocity => (0, 127, 127, None),
        Detail::Prob => (0, 100, 100, None),
        // two octaves either way of the voice's pitch
        Detail::Note => (36, 84, 60, Some(60)),
        Detail::Ratchet => (0, 5, 0, None),
        Detail::Shift => (-7, 7, 0, Some(0)),
        Detail::Lock(_) => (0, 127, 0, None),
    }
}

/// The step's value, or None when a lock slot holds another parameter (or none).
fn get(d: &StepData, detail: Detail, lock: Option<u16>) -> Option<i32> {
    Some(match detail {
        Detail::Velocity => d.velocity as i32,
        Detail::Prob => d.prob as i32,
        Detail::Note => d.note as i32,
        Detail::Ratchet => d.flam as i32,
        Detail::Shift => d.shift as i32,
        Detail::Lock(slot) => {
            if lock.is_none() || d.dest[slot] != lock {
                return None;
            }
            d.value[slot] as i32
        }
    })
}

fn set(d: &mut StepData, detail: Detail, value: Option<i32>, lock: Option<u16>) {
    let (lo, hi, default, _) = range(detail);
    let v = value.unwrap_or(default).clamp(lo, hi);
    match detail {
        Detail::Velocity => d.velocity = v as u8,
        Detail::Prob => d.prob = v as u8,
        Detail::Note => d.note = v as u8,
        Detail::Ratchet => d.flam = v as u8,
        Detail::Shift => d.shift = v as i8,
        Detail::Lock(slot) => {
            // no value clears the lock
            d.dest[slot] = if value.is_some() { lock } else { None };
            d.value[slot] = v as u8;
        }
    }
}

fn text(detail: Detail, v: i32) -> String {
    match detail {
        Detail::Prob => format!("{v}%"),
        Detail::Note => sequencer::note_name(v as u8),
        Detail::Ratchet => FLAM_NAMES[v.clamp(0, 5) as usize].to_string(),
        Detail::Shift => format!("{v:+}"),
        _ => v.to_string(),
    }
}

fn target_name(param: u16) -> String {
    MOD_TARGETS.iter().find(|t| t.param == param).map_or("?".to_string(), |t| t.long.to_string())
}

/// The parameter a lock slot shows: the one picked, or the one the track's steps use most.
fn lock_target(st: &mut State, pattern: &[u8], track: usize, slot: usize) -> Option<u16> {
    let choices = sequencer::dest_choices(track);
    if let Some(t) = st.lane_lock[slot] {
        if choices.contains(&Some(t)) {
            return Some(t);
        }
    }
    let mut counts: Vec<(u16, usize)> = Vec::new();
    for step in 0..64 {
        if let Some(p) = seq::step(pattern, track, step).dest[slot] {
            match counts.iter_mut().find(|(q, _)| *q == p) {
                Some((_, n)) => *n += 1,
                None => counts.push((p, 1)),
            }
        }
    }
    let picked = counts.iter().max_by_key(|(_, n)| *n).map(|(p, _)| *p).or_else(|| choices.iter().flatten().next().copied());
    st.lane_lock[slot] = picked;
    picked
}

/// Draws the tabs at `tabs_y` and the strip in `area`, whose steps line up with the grid's
/// (`cell_w` wide from `area.left()`).
pub(super) fn draw(ui: &mut Ui, st: &mut State, tabs_y: f32, area: Rect, cell_w: f32) {
    let painter = ui.painter().clone();
    let params = st.params.clone();
    let seq_state = &params.seq;
    let pattern_nr = seq_state.pending.load(Ordering::Acquire) as usize % NUM_PATTERNS;
    let track = st.last_track.min(6);
    let detail = DETAILS[st.lane_detail].1;
    let Ok(pattern) = seq_state.patterns.lock().map(|p| p[pattern_nr].clone()) else { return };
    let lock = match detail {
        Detail::Lock(slot) => lock_target(st, &pattern, track, slot),
        _ => None,
    };

    // ---- tabs, the track it edits and, for locks, the parameter
    painter.text(Pos2::new(22.0, tabs_y + 11.0), Align2::LEFT_CENTER, lanes::LANE_NAMES[track], FontId::proportional(13.0), LED_RED);
    for (i, (name, _)) in DETAILS.iter().enumerate() {
        let rect = Rect::from_min_size(Pos2::new(area.left() + i as f32 * 78.0, tabs_y), Vec2::new(74.0, 22.0));
        if lanes::small_button(ui, rect, name, i == st.lane_detail).on_hover_text(HELP[i]).clicked() {
            st.lane_detail = i;
        }
    }
    let reset = Rect::from_min_size(Pos2::new(area.right() - 64.0, tabs_y), Vec2::new(64.0, 22.0));
    let reset_all = lanes::small_button(ui, reset, "RESET", false)
        .on_hover_text("this setting back to normal on every step of the track (locks: removed)")
        .clicked();
    if let Detail::Lock(slot) = detail {
        let choices = sequencer::dest_choices(track);
        let pos = choices.iter().position(|c| *c == lock).unwrap_or(0) as i32;
        let x = area.left() + 7.0 * 78.0 + 6.0;
        let prev = lanes::small_button(ui, Rect::from_min_size(Pos2::new(x, tabs_y), Vec2::new(22.0, 22.0)), "◀", false).clicked();
        let next = lanes::small_button(ui, Rect::from_min_size(Pos2::new(x + 140.0, tabs_y), Vec2::new(22.0, 22.0)), "▶", false).clicked();
        let name = lock.map_or("—".to_string(), target_name);
        painter.text(Pos2::new(x + 81.0, tabs_y + 11.0), Align2::CENTER_CENTER, name, FontId::proportional(12.0), LCD_ON);
        if prev || next {
            // "off" (the first choice) is not a parameter to draw
            let n = (pos + if next { 1 } else { -1 }).clamp(1, choices.len() as i32 - 1) as usize;
            st.lane_lock[slot] = choices.get(n).copied().flatten();
        }
    }

    // ---- the bars
    let (lo, hi, _, centre) = range(detail);
    let to_y = |v: i32| area.bottom() - (v - lo) as f32 / (hi - lo) as f32 * area.height();
    let length = seq::track_length(&pattern, track) as usize;
    painter.rect_filled(area, 3.0, Color32::from_rgb(16, 16, 18));
    if let Some(c) = centre {
        painter.line_segment([Pos2::new(area.left(), to_y(c)), Pos2::new(area.right(), to_y(c))], Stroke::new(1.0_f32, LABEL_DIM));
    }
    if detail == Detail::Ratchet {
        for v in 1..=5 {
            painter.line_segment([Pos2::new(area.left(), to_y(v)), Pos2::new(area.right(), to_y(v))], Stroke::new(0.5_f32, PANEL_EDGE));
        }
    }
    for step in 0..64 {
        let x0 = area.left() + step as f32 * cell_w;
        let cell = Rect::from_min_max(Pos2::new(x0 + 1.5, area.top()), Pos2::new(x0 + cell_w - 1.5, area.bottom()));
        if step % 16 == 0 && step > 0 {
            painter.line_segment([Pos2::new(x0, area.top()), Pos2::new(x0, area.bottom())], Stroke::new(1.5_f32, PANEL));
        }
        if !seq::step_on(&pattern, track, step) {
            continue;
        }
        let d = seq::step(&pattern, track, step);
        let color = if step < length { LED_RED } else { LED_RED.gamma_multiply(0.35) };
        match get(&d, detail, lock) {
            Some(v) => {
                let base = to_y(centre.unwrap_or(lo));
                let top = to_y(v);
                let bar = Rect::from_min_max(Pos2::new(cell.left(), top.min(base)), Pos2::new(cell.right(), top.max(base).max(top.min(base) + 2.0)));
                painter.rect_filled(bar, 1.5, color.gamma_multiply(0.75));
                painter.rect_filled(Rect::from_min_max(Pos2::new(cell.left(), top - 1.5), Pos2::new(cell.right(), top + 1.5)), 1.0, color);
            }
            // on, but without this lock (or with another parameter in its slot)
            None => {
                let other = matches!(detail, Detail::Lock(s) if d.dest[s].is_some());
                painter.circle_filled(Pos2::new(cell.center().x, area.bottom() - 5.0), 2.5, if other { LABEL_DIM } else { PANEL_EDGE });
            }
        }
    }

    // ---- drawing: every step crossed takes the value under the pointer; only steps that are on
    let resp = ui.allocate_rect(area, Sense::click_and_drag());
    let step_at = |pos: Pos2| (((pos.x - area.left()) / cell_w) as usize).min(63);
    let value_at = |pos: Pos2| (lo as f32 + (area.bottom() - pos.y) / area.height() * (hi - lo) as f32).round() as i32;
    let mut edits: Vec<(usize, Option<i32>)> = Vec::new();
    if reset_all {
        edits.extend((0..64).map(|s| (s, None)));
    }
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.secondary_clicked() {
            edits.push((step_at(pos), None));
        } else if resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down()) {
            let (step, v) = (step_at(pos), value_at(pos).clamp(lo, hi));
            // fill the steps between this frame and the last, so a quick stroke leaves no gaps
            let (from, from_v) = st.lane_draw.unwrap_or((step, v));
            let n = from.abs_diff(step).max(1);
            for k in 0..=from.abs_diff(step) {
                let s = if step >= from { from + k } else { from - k };
                let t = k as f32 / n as f32;
                edits.push((s, Some((from_v as f32 + (v - from_v) as f32 * t).round() as i32)));
            }
            st.lane_draw = Some((step, v));
        }
    }
    if !ui.input(|i| i.pointer.primary_down()) {
        st.lane_draw = None;
    }
    if !edits.is_empty() && (lock.is_some() || !matches!(detail, Detail::Lock(_))) {
        if let Ok(mut p) = seq_state.patterns.lock() {
            for (step, value) in edits {
                if !seq::step_on(&p[pattern_nr], track, step) && !reset_all {
                    continue;
                }
                let mut d = seq::step(&p[pattern_nr], track, step);
                // RESET and right-click only clear this lock's own parameter
                if let (Detail::Lock(slot), None) = (detail, value) {
                    if d.dest[slot] != lock {
                        continue;
                    }
                }
                set(&mut d, detail, value, lock);
                seq::set_step(&mut p[pattern_nr], track, step, d);
            }
        }
        seq_state.mark_dirty(pattern_nr);
    }

    // ---- what is under the pointer
    if let Some(pos) = resp.hover_pos() {
        let step = step_at(pos);
        let d = seq::step(&pattern, track, step);
        let value = if seq::step_on(&pattern, track, step) {
            get(&d, detail, lock).map_or("—".to_string(), |v| text(detail, v))
        } else {
            "step off".to_string()
        };
        painter.text(
            Pos2::new(WIDTH as f32 / 2.0, HEIGHT as f32 - 20.0),
            Align2::CENTER_CENTER,
            format!("{} · step {} · {}: {} · drag to draw · right-click resets the step", lanes::LANE_NAMES[track], step + 1, DETAILS[st.lane_detail].0.to_lowercase(), value),
            FontId::proportional(11.0),
            LABEL_DIM,
        );
    }
}
