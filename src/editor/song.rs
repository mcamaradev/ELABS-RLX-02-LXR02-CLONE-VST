//! COPY/CLR page (clear or copy tracks and patterns, like the LXR's copy/clear functions, plus
//! doubling and halving a pattern) and SONG
//! page (the firmware's song mode: a list of patterns played in order).

use super::*;
use crate::seq::{self, NUM_PATTERNS, NUM_TRACKS};

const COPY_ACTIONS: [&str; 6] = ["clr trk", "clr pat", "cpy pat", "cpy trk", "dbl pat", "hlf pat"];
const LAST_ACTION: usize = COPY_ACTIONS.len() - 1;

pub(super) struct CopyState {
    pub action: usize,
    /// destination pattern of "cpy pat"
    pub to_pattern: usize,
    /// destination track of "cpy trk"
    pub to_track: usize,
    pub message: Option<(String, f64)>,
}

impl Default for CopyState {
    fn default() -> Self {
        Self { action: 0, to_pattern: 1, to_track: 1, message: None }
    }
}

fn shown_pattern(st: &State) -> usize {
    st.params.seq.pending.load(Ordering::Acquire) as usize % NUM_PATTERNS
}

/// Track names short enough for the display.
const TRACK_SHORT: [&str; 7] = ["D1", "D2", "D3", "SN", "CY", "CH", "OH"];

pub(super) fn copy_rows(st: &State) -> [String; 2] {
    let c = &st.copy;
    let track = sequencer::track(st);
    let pattern = shown_pattern(st) + 1;
    let row1 = match c.action {
        0 => format!("P{pattern:02} {:<9}ok?", TRACK_SHORT[track]),
        1 => format!("P{pattern:02}{:>13}", "ok?"),
        2 => format!("P{pattern:02} > P{:02}{:>6}", c.to_pattern + 1, "ok?"),
        3 => format!("P{pattern:02} {} > {}{:>5}", TRACK_SHORT[track], TRACK_SHORT[c.to_track], "ok?"),
        4 => format!("P{pattern:02} len x2{:>6}", "ok?"),
        _ => format!("P{pattern:02} len /2{:>6}", "ok?"),
    };
    [format!("{:<16}", COPY_ACTIONS[c.action]), row1]
}

pub(super) fn copy_caption(st: &State, now: f64) -> String {
    match &st.copy.message {
        Some((text, until)) if now < *until => text.clone(),
        _ => "knob 1 action · knob 2 to pattern · knob 3 to track · click DATA or display to do it".to_string(),
    }
}

/// Doubles (duplicate: every track plays its steps twice over twice the length) or halves the
/// shown pattern's tracks.
pub(super) fn resize_pattern(st: &mut State, double: bool, now: f64) {
    let seq_state = &st.params.seq;
    let pattern = shown_pattern(st);
    let changed = {
        let Ok(mut p) = seq_state.patterns.lock() else { return };
        if double { seq::double_pattern(&mut p[pattern]) } else { seq::halve_pattern(&mut p[pattern]) }
    };
    seq_state.mark_dirty(pattern);
    let text = match (changed, double) {
        (0, true) => format!("P{:02}: every track is already longer than 32 steps", pattern + 1),
        (0, false) => format!("P{:02}: every track is already 1 step long", pattern + 1),
        (n, true) if n < NUM_TRACKS => format!("P{:02} doubled ({} tracks longer than 32 steps kept)", pattern + 1, NUM_TRACKS - n),
        (_, true) => format!("P{:02} doubled", pattern + 1),
        (_, false) => format!("P{:02} halved (the second half stays, lengthen to hear it again)", pattern + 1),
    };
    st.copy.message = Some((text, now + 3.0));
}

fn copy_apply(st: &mut State, now: f64) {
    if st.copy.action >= 4 {
        return resize_pattern(st, st.copy.action == 4, now);
    }
    let seq_state = &st.params.seq;
    let pattern = shown_pattern(st);
    let track = sequencer::track(st);
    let c = &st.copy;
    let text = {
        let Ok(mut p) = seq_state.patterns.lock() else { return };
        match c.action {
            0 => {
                seq::clear_track(&mut p[pattern], track);
                format!("cleared {} in P{:02}", VOICE_LABELS[track], pattern + 1)
            }
            1 => {
                seq::clear_pattern(&mut p[pattern]);
                format!("cleared P{:02}", pattern + 1)
            }
            2 => {
                let src = p[pattern].clone();
                p[c.to_pattern] = src;
                seq_state.mark_dirty(c.to_pattern);
                format!("copied P{:02} to P{:02}", pattern + 1, c.to_pattern + 1)
            }
            _ => {
                // 3: cpy trk
                seq::copy_track(&mut p[pattern], track, c.to_track);
                format!("copied {} to {}", VOICE_LABELS[track], VOICE_LABELS[c.to_track])
            }
        }
    };
    seq_state.mark_dirty(pattern);
    st.copy.message = Some((text, now + 3.0));
}

pub(super) fn copy_knob(ui: &mut Ui, st: &mut State, column: usize, center: Pos2, radius: f32, resp: &Response) {
    let c = &st.copy;
    let turn = match column {
        0 => Some(c.action as f32 / LAST_ACTION as f32),
        1 => Some(c.to_pattern as f32 / (NUM_PATTERNS - 1) as f32),
        2 => Some(c.to_track as f32 / (NUM_TRACKS - 1) as f32),
        _ => None,
    };
    knob_face(ui.painter(), center, radius, turn, false);
    let scroll = scroll_steps(ui, resp, &mut st.scroll_accum);
    let delta = drag_steps(resp, &mut st.drag_accum, if column == 1 { 6.0 } else { 20.0 }) + scroll;
    let c = &mut st.copy;
    match column {
        0 => c.action = (c.action as i32 + delta).clamp(0, LAST_ACTION as i32) as usize,
        1 => c.to_pattern = (c.to_pattern as i32 + delta).clamp(0, NUM_PATTERNS as i32 - 1) as usize,
        2 => c.to_track = (c.to_track as i32 + delta).clamp(0, NUM_TRACKS as i32 - 1) as usize,
        _ => {}
    }
}

pub(super) fn copy_encoder(ui: &mut Ui, st: &mut State, center: Pos2, radius: f32, resp: &Response) {
    knob_face(ui.painter(), center, radius, Some(st.copy.action as f32 / LAST_ACTION as f32), true);
    let delta = drag_steps(resp, &mut st.drag_accum, 20.0) + scroll_steps(ui, resp, &mut st.scroll_accum);
    st.copy.action = (st.copy.action as i32 + delta).clamp(0, LAST_ACTION as i32) as usize;
    if resp.clicked() {
        let now = ui.input(|i| i.time);
        copy_apply(st, now);
    }
}

pub(super) fn copy_click_display(st: &mut State, now: f64) {
    copy_apply(st, now);
}

// ---------------------------------------------------------------- SONG page

fn song(st: &State) -> Vec<u8> {
    st.params.seq.song.lock().map(|s| s.clone()).unwrap_or_default()
}

pub(super) fn song_rows(st: &State) -> [String; 2] {
    let seq_state = &st.params.seq;
    let song = song(st);
    let on = if seq_state.song_on.load(Ordering::Acquire) { "on" } else { "off" };
    let end = if seq_state.song_loop.load(Ordering::Acquire) { "loop" } else { "stop" };
    let cursor = st.song_cursor.min(song.len());
    let row0 = format!("SONG {on:<3} {end:<4}{:>3}", song.len());
    // four entries from the one before the cursor; the cursor's is marked, "--" is the free slot
    let first = cursor.saturating_sub(1);
    let mut row1 = String::new();
    for i in first..first + 4 {
        let text = match song.get(i) {
            Some(p) => format!("{:02}", p + 1),
            None if i == song.len() => "--".to_string(),
            None => "  ".to_string(),
        };
        row1.push_str(&if i == cursor { format!(">{text} ") } else { format!(" {text} ") });
    }
    [row0, row1]
}

pub(super) fn song_caption(st: &State) -> String {
    format!(
        "position {} · knob 1 position · 2 pattern · 3 song on/off · 4 loop/stop · DATA click deletes · plays while the sequencer runs",
        st.song_cursor.min(song(st).len()) + 1
    )
}

fn edit_song(st: &State, f: impl FnOnce(&mut Vec<u8>)) {
    if let Ok(mut s) = st.params.seq.song.lock() {
        f(&mut s);
    }
    st.params.seq.song_dirty.store(true, Ordering::Release);
}

pub(super) fn song_knob(ui: &mut Ui, st: &mut State, column: usize, center: Pos2, radius: f32, resp: &Response) {
    let seq_state = &st.params.seq;
    let song = song(st);
    let cursor = st.song_cursor.min(song.len());
    let turn = match column {
        0 => Some(cursor as f32 / 63.0),
        1 => Some(song.get(cursor).map_or(0.0, |p| *p as f32 / 63.0)),
        2 => Some(if seq_state.song_on.load(Ordering::Acquire) { 1.0 } else { 0.0 }),
        _ => Some(if seq_state.song_loop.load(Ordering::Acquire) { 1.0 } else { 0.0 }),
    };
    knob_face(ui.painter(), center, radius, turn, false);
    let delta = drag_steps(resp, &mut st.drag_accum, if column == 1 { 6.0 } else { 15.0 }) + scroll_steps(ui, resp, &mut st.scroll_accum);
    match column {
        0 => st.song_cursor = (cursor as i32 + delta).clamp(0, song.len().min(62) as i32) as usize,
        1 if delta != 0 => {
            // turning on the free slot adds an entry there
            edit_song(st, |s| {
                if cursor == s.len() {
                    let last = s.last().copied().unwrap_or(0);
                    s.push(last);
                }
                s[cursor] = (s[cursor] as i32 + delta).clamp(0, NUM_PATTERNS as i32 - 1) as u8;
            });
        }
        2 if resp.clicked() || delta != 0 => {
            seq_state.song_on.fetch_xor(true, Ordering::AcqRel);
            seq_state.song_dirty.store(true, Ordering::Release);
        }
        3 if resp.clicked() || delta != 0 => {
            seq_state.song_loop.fetch_xor(true, Ordering::AcqRel);
            seq_state.song_dirty.store(true, Ordering::Release);
        }
        _ => {}
    }
}

pub(super) fn song_encoder(ui: &mut Ui, st: &mut State, center: Pos2, radius: f32, resp: &Response) {
    let song = song(st);
    let cursor = st.song_cursor.min(song.len());
    knob_face(ui.painter(), center, radius, Some(cursor as f32 / 63.0), true);
    let delta = drag_steps(resp, &mut st.drag_accum, 8.0) + scroll_steps(ui, resp, &mut st.scroll_accum);
    st.song_cursor = (cursor as i32 + delta).clamp(0, song.len().min(62) as i32) as usize;
    if resp.clicked() && cursor < song.len() {
        edit_song(st, |s| {
            s.remove(cursor);
        });
    }
}
