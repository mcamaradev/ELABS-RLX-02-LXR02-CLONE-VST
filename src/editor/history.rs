//! Undo and redo (⌘Z / ⇧⌘Z, or the UNDO and REDO buttons) for what the panel changes: the sound
//! (knobs, faders, morph, loaded kits), the patterns, the mutes and the song.
//!
//! Every mouse gesture (a click, a drag, a run of scrolling) is one step: the state is noted when
//! it starts and compared when the next click starts or the input has settled. Only what the gesture changed is kept, so undoing
//! it leaves alone what the host automation or REC recording changed meanwhile.

use super::*;
use crate::seq::NUM_PATTERNS;

/// steps kept
const LIMIT: usize = 100;
/// seconds without input after which a gesture is over (scrolling has momentum)
const SETTLE: f64 = 0.4;

struct Snapshot {
    sound: Vec<i32>,
    faders: Vec<f32>,
    morph: i32,
    kit_name: String,
    patterns: Vec<Vec<u8>>,
    mutes: u8,
    song: Vec<u8>,
}

/// What one gesture changed, as (before, after) pairs.
#[derive(Default)]
struct Change {
    sound: Vec<(usize, i32, i32)>,
    faders: Vec<(usize, f32, f32)>,
    morph: Option<(i32, i32)>,
    kit_name: Option<(String, String)>,
    /// pattern number and its changed bytes (offset, before, after)
    patterns: Vec<(usize, Vec<(u16, u8, u8)>)>,
    mutes: Option<(u8, u8)>,
    song: Option<(Vec<u8>, Vec<u8>)>,
}

impl Change {
    fn is_empty(&self) -> bool {
        self.sound.is_empty()
            && self.faders.is_empty()
            && self.morph.is_none()
            && self.kit_name.is_none()
            && self.patterns.is_empty()
            && self.mutes.is_none()
            && self.song.is_none()
    }

    /// A few words for the message: "kit Badonka", "pattern 03", "sound"...
    fn describe(&self, undo: bool) -> String {
        let mut parts = Vec::new();
        if let Some((before, after)) = &self.kit_name {
            parts.push(format!("kit {}", if undo { before } else { after }));
        } else if !self.sound.is_empty() || !self.faders.is_empty() || self.morph.is_some() {
            parts.push(match self.sound.as_slice() {
                [(i, _, _)] => self.sound_name(*i),
                _ => "sound".to_string(),
            });
        }
        for (nr, _) in self.patterns.iter().take(3) {
            parts.push(format!("pattern {:02}", nr + 1));
        }
        if self.mutes.is_some() {
            parts.push("mutes".to_string());
        }
        if self.song.is_some() {
            parts.push("song".to_string());
        }
        parts.join(", ")
    }

    fn sound_name(&self, i: usize) -> String {
        crate::param_table::PARAMS.get(i).map_or("sound".to_string(), |p| p.name.to_string())
    }
}

#[derive(Default)]
pub(super) struct History {
    undo: Vec<Change>,
    redo: Vec<Change>,
    /// state when the gesture in progress started
    before: Option<Snapshot>,
    last_input: f64,
    message: Option<(String, f64)>,
}

impl History {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.before.is_some()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

fn snapshot(st: &State) -> Snapshot {
    let params = &st.params;
    let seq_state = &params.seq;
    Snapshot {
        sound: params.sound.iter().map(|p| p.value()).collect(),
        faders: params.faders.iter().map(|p| p.value()).collect(),
        morph: params.morph.value(),
        kit_name: params.kit_name.lock().map(|n| n.clone()).unwrap_or_default(),
        patterns: seq_state.patterns.lock().map(|p| p.clone()).unwrap_or_default(),
        mutes: seq_state.mutes.load(Ordering::Acquire),
        song: seq_state.song.lock().map(|s| s.clone()).unwrap_or_default(),
    }
}

fn diff(a: &Snapshot, b: &Snapshot) -> Change {
    let pairs = |x: &[i32], y: &[i32]| -> Vec<(usize, i32, i32)> {
        x.iter().zip(y).enumerate().filter(|(_, (p, q))| p != q).map(|(i, (p, q))| (i, *p, *q)).collect()
    };
    let mut patterns = Vec::new();
    if a.patterns.len() == b.patterns.len() {
        for (nr, (p, q)) in a.patterns.iter().zip(&b.patterns).enumerate() {
            let bytes: Vec<(u16, u8, u8)> =
                p.iter().zip(q).enumerate().filter(|(_, (x, y))| x != y).map(|(i, (x, y))| (i as u16, *x, *y)).collect();
            if !bytes.is_empty() {
                patterns.push((nr, bytes));
            }
        }
    }
    Change {
        sound: pairs(&a.sound, &b.sound),
        faders: a.faders.iter().zip(&b.faders).enumerate().filter(|(_, (p, q))| p != q).map(|(i, (p, q))| (i, *p, *q)).collect(),
        morph: (a.morph != b.morph).then_some((a.morph, b.morph)),
        kit_name: (a.kit_name != b.kit_name).then(|| (a.kit_name.clone(), b.kit_name.clone())),
        patterns,
        mutes: (a.mutes != b.mutes).then_some((a.mutes, b.mutes)),
        song: (a.song != b.song).then(|| (a.song.clone(), b.song.clone())),
    }
}

/// Ends the gesture in progress, keeping what it changed as an undo step.
fn commit(st: &mut State) {
    let Some(before) = st.history.before.take() else { return };
    let change = diff(&before, &snapshot(st));
    if change.is_empty() {
        return;
    }
    let h = &mut st.history;
    h.undo.push(change);
    if h.undo.len() > LIMIT {
        h.undo.remove(0);
    }
    h.redo.clear();
}

/// Puts back the "before" (undo) or "after" (redo) side of a change.
fn apply(setter: &ParamSetter, st: &State, c: &Change, undo: bool) {
    fn side<T>(undo: bool, before: T, after: T) -> T {
        if undo { before } else { after }
    }
    let params = &st.params;
    for &(i, before, after) in &c.sound {
        let p = &params.sound[i];
        setter.begin_set_parameter(p);
        setter.set_parameter(p, side(undo, before, after));
        setter.end_set_parameter(p);
    }
    for &(i, before, after) in &c.faders {
        let p = &params.faders[i];
        setter.begin_set_parameter(p);
        setter.set_parameter(p, side(undo, before, after));
        setter.end_set_parameter(p);
    }
    if let Some((before, after)) = c.morph {
        setter.begin_set_parameter(&params.morph);
        setter.set_parameter(&params.morph, side(undo, before, after));
        setter.end_set_parameter(&params.morph);
    }
    if let Some((before, after)) = &c.kit_name {
        if let Ok(mut n) = params.kit_name.lock() {
            *n = side(undo, before, after).clone();
        }
    }
    let seq_state = &params.seq;
    if let Ok(mut patterns) = seq_state.patterns.lock() {
        for (nr, bytes) in &c.patterns {
            if *nr >= NUM_PATTERNS {
                continue;
            }
            for &(at, before, after) in bytes {
                patterns[*nr][at as usize] = side(undo, before, after);
            }
        }
    }
    for (nr, _) in &c.patterns {
        seq_state.mark_dirty(*nr);
    }
    if let Some((before, after)) = c.mutes {
        seq_state.mutes.store(side(undo, before, after), Ordering::Release);
    }
    if let Some((before, after)) = &c.song {
        if let Ok(mut s) = seq_state.song.lock() {
            *s = side(undo, before, after).clone();
        }
        seq_state.song_dirty.store(true, Ordering::Release);
    }
}

pub(super) fn undo(setter: &ParamSetter, st: &mut State, now: f64) {
    commit(st);
    let Some(change) = st.history.undo.pop() else {
        st.history.message = Some(("nothing to undo".to_string(), now + 1.5));
        return;
    };
    apply(setter, st, &change, true);
    st.history.message = Some((format!("undo: {}", change.describe(true)), now + 2.0));
    st.history.redo.push(change);
}

pub(super) fn redo(setter: &ParamSetter, st: &mut State, now: f64) {
    commit(st);
    let Some(change) = st.history.redo.pop() else {
        st.history.message = Some(("nothing to redo".to_string(), now + 1.5));
        return;
    };
    apply(setter, st, &change, false);
    st.history.message = Some((format!("redo: {}", change.describe(false)), now + 2.0));
    st.history.undo.push(change);
}

/// Called every frame before the panel is drawn: notes the state when a gesture starts, keeps
/// the step when it is over, and handles the keyboard shortcuts.
pub(super) fn begin_frame(ctx: &egui::Context, setter: &ParamSetter, st: &mut State) {
    let typing = ctx.wants_keyboard_input();
    let (now, pressed, active, undo_key, redo_key) = ctx.input(|i| {
        let active = i.pointer.any_down()
            || i.pointer.any_released()
            || i.raw_scroll_delta != Vec2::ZERO
            || i.smooth_scroll_delta != Vec2::ZERO;
        let z = i.modifiers.command && i.key_pressed(egui::Key::Z);
        let y = i.modifiers.command && i.key_pressed(egui::Key::Y);
        (i.time, i.pointer.any_pressed(), active, z && !i.modifiers.shift, (z && i.modifiers.shift) || y)
    });
    // a new click is a new step, however quickly it follows the last one
    if pressed {
        commit(st);
    }
    if active {
        st.history.last_input = now;
        if st.history.before.is_none() {
            st.history.before = Some(snapshot(st));
        }
    } else if st.history.before.is_some() && now - st.history.last_input > SETTLE {
        commit(st);
    }
    // a text field (kit name, search) has its own undo
    if !typing {
        if undo_key {
            undo(setter, st, now);
        } else if redo_key {
            redo(setter, st, now);
        }
    }
}

/// The last undo or redo, at the bottom of the window for a moment.
pub(super) fn draw_message(ui: &mut Ui, st: &State) {
    let now = ui.input(|i| i.time);
    let Some((text, until)) = &st.history.message else { return };
    if now >= *until {
        return;
    }
    let font = FontId::proportional(13.0);
    let galley = ui.painter().layout_no_wrap(text.clone(), font, Color32::WHITE);
    let size = galley.size() + Vec2::new(20.0, 10.0);
    let rect = Rect::from_center_size(Pos2::new(WIDTH as f32 / 2.0, HEIGHT as f32 - 46.0), size);
    let painter = ui.painter();
    painter.rect_filled(rect, 5.0, Color32::from_rgb(30, 8, 10));
    painter.rect_stroke(rect, 5.0, Stroke::new(1.0_f32, LED_RED), StrokeKind::Inside);
    painter.galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
}

/// UNDO and REDO buttons; `rects` are where to draw them.
pub(super) fn buttons(ui: &mut Ui, setter: &ParamSetter, st: &mut State, rects: [Rect; 2], font: f32) {
    let now = ui.input(|i| i.time);
    for (k, rect) in rects.into_iter().enumerate() {
        let (label, help, available) = if k == 0 {
            ("UNDO", "undo the last change (⌘Z)", st.history.can_undo())
        } else {
            ("REDO", "redo what was undone (⇧⌘Z)", st.history.can_redo())
        };
        let resp = ui.allocate_rect(rect, Sense::click()).on_hover_text(help);
        let painter = ui.painter();
        painter.rect_filled(rect, 3.0, if resp.hovered() && available { BUTTON } else { BUTTON_DIM });
        painter.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(font), if available { LABEL } else { LABEL_DIM });
        if resp.clicked() {
            if k == 0 {
                undo(setter, st, now);
            } else {
                redo(setter, st, now);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(sound: Vec<i32>, patterns: Vec<Vec<u8>>) -> Snapshot {
        Snapshot { sound, faders: vec![1.0], morph: 0, kit_name: "A".into(), patterns, mutes: 0, song: vec![] }
    }

    #[test]
    fn a_change_keeps_only_what_differs() {
        let a = snap(vec![1, 2, 3], vec![vec![0; 4], vec![9; 4]]);
        let mut b = snap(vec![1, 5, 3], vec![vec![0, 7, 0, 0], vec![9; 4]]);
        b.kit_name = "B".into();
        let c = diff(&a, &b);
        assert_eq!(c.sound, vec![(1, 2, 5)]);
        assert_eq!(c.patterns, vec![(0, vec![(1, 0, 7)])]);
        assert_eq!(c.kit_name, Some(("A".into(), "B".into())));
        assert!(c.faders.is_empty() && c.mutes.is_none() && c.song.is_none() && c.morph.is_none());
        assert_eq!(c.describe(true), "kit A, pattern 01");
        assert!(diff(&a, &a).is_empty());
    }
}
