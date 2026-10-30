//! LANES view: every track of the shown pattern at once, all 64 steps. Click turns a step on or
//! off, dragging paints several, right-click (or cmd-click) opens the step's settings. Each lane has
//! its mute and length; brightness shows the velocity. DUPLICATE and HALVE double or halve the
//! pattern's length. Clicking a lane picks the track the detail strip below shows
//! (lane_detail.rs).

use super::*;
use crate::seq::{self, NUM_PATTERNS, NUM_TRACKS};

pub(super) const LANE_NAMES: [&str; 7] = ["DRUM1", "DRUM2", "DRUM3", "SNARE", "CLP/CYM", "CL HH", "OP HH"];
const STEPS: usize = 64;

fn shown_pattern(st: &State) -> usize {
    st.params.seq.pending.load(Ordering::Acquire) as usize % NUM_PATTERNS
}

pub(super) fn small_button(ui: &mut Ui, rect: Rect, text: &str, lit: bool) -> Response {
    let resp = ui.allocate_rect(rect, Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, if lit { LED_RED } else if resp.hovered() { BUTTON } else { BUTTON_DIM });
    painter.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(12.0), if lit { PANEL } else { LABEL });
    resp
}

/// Returns true when the view should close.
pub(super) fn draw(ui: &mut Ui, setter: &ParamSetter, st: &mut State) -> bool {
    let (w, h) = (WIDTH as f32, HEIGHT as f32);
    let painter = ui.painter().clone();
    painter.rect_stroke(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h)).shrink(4.0), 10.0, Stroke::new(2.0_f32, PANEL_EDGE), StrokeKind::Inside);
    let params = st.params.clone();
    let seq_state = &params.seq;
    let pattern = shown_pattern(st);

    // ---- header: back, pattern, transport-ish toggles
    if small_button(ui, Rect::from_min_size(Pos2::new(16.0, 14.0), Vec2::new(92.0, 26.0)), "◀ PANEL", false).clicked() {
        return true;
    }
    let prev = small_button(ui, Rect::from_min_size(Pos2::new(124.0, 14.0), Vec2::new(28.0, 26.0)), "◀", false).clicked();
    painter.text(Pos2::new(196.0, 27.0), Align2::CENTER_CENTER, format!("PATTERN {:02}", pattern + 1), FontId::proportional(15.0), LCD_ON);
    let next = small_button(ui, Rect::from_min_size(Pos2::new(240.0, 14.0), Vec2::new(28.0, 26.0)), "▶", false).clicked();
    if prev || next {
        let nr = (pattern as i32 + if next { 1 } else { -1 }).rem_euclid(NUM_PATTERNS as i32) as usize;
        seq_state.request_pattern(nr);
        st.pat_sel = nr;
    }
    let active = seq_state.active.load(Ordering::Acquire) as usize;
    if active != pattern {
        painter.text(Pos2::new(282.0, 27.0), Align2::LEFT_CENTER, format!("(playing {:02})", active + 1), FontId::proportional(11.0), LABEL_DIM);
    }
    let now = ui.input(|i| i.time);
    // the Maestro hides behind this
    let robot = Rect::from_min_size(Pos2::new(362.0, 14.0), Vec2::new(30.0, 26.0));
    let robot_resp = ui.allocate_rect(robot, Sense::click()).on_hover_text("MAESTRO");
    if robot_resp.clicked() {
        st.maestro_open = !st.maestro_open;
    }
    let maestro_on = params.maestro.on.load(Ordering::Acquire);
    let robot_color = if st.maestro_open || robot_resp.hovered() { LABEL } else { BUTTON_DIM };
    maestro_panel::robot(&painter, robot.center(), 22.0, robot_color, if maestro_on { LED_RED } else { PANEL });
    if small_button(ui, Rect::from_min_size(Pos2::new(400.0, 14.0), Vec2::new(116.0, 26.0)), "DUPLICATE ×2", false)
        .on_hover_text("every track twice as long, its steps played twice (tracks longer than 32 steps stay)")
        .clicked()
    {
        song::resize_pattern(st, true, now);
    }
    if small_button(ui, Rect::from_min_size(Pos2::new(524.0, 14.0), Vec2::new(116.0, 26.0)), "HALVE ÷2", false)
        .on_hover_text("every track half as long; the second half stays in the pattern, lengthen the track to hear it again")
        .clicked()
    {
        song::resize_pattern(st, false, now);
    }
    history::buttons(
        ui,
        setter,
        st,
        [Rect::from_min_size(Pos2::new(650.0, 14.0), Vec2::new(50.0, 26.0)), Rect::from_min_size(Pos2::new(704.0, 14.0), Vec2::new(50.0, 26.0))],
        12.0,
    );
    let on = params.seq_on.value();
    if small_button(ui, Rect::from_min_size(Pos2::new(w - 236.0, 14.0), Vec2::new(104.0, 26.0)), "PLAY", on).clicked() {
        setter.begin_set_parameter(&params.seq_on);
        setter.set_parameter(&params.seq_on, !on);
        setter.end_set_parameter(&params.seq_on);
    }
    let rec = seq_state.record.load(Ordering::Acquire);
    if small_button(ui, Rect::from_min_size(Pos2::new(w - 124.0, 14.0), Vec2::new(104.0, 26.0)), "REC", rec).clicked() {
        seq_state.record.store(!rec, Ordering::Release);
    }

    // ---- grid geometry
    let label_w = 132.0;
    // the lanes, then the detail strip of the selected track below them
    let lane_h = 31.0;
    let grid = Rect::from_min_max(Pos2::new(16.0 + label_w, 62.0), Pos2::new(w - 16.0, 62.0 + NUM_TRACKS as f32 * lane_h));
    let cell_w = grid.width() / STEPS as f32;
    // bar numbers
    for bar in 0..4 {
        painter.text(
            Pos2::new(grid.left() + (bar * 16) as f32 * cell_w + 2.0, grid.top() - 9.0),
            Align2::LEFT_CENTER,
            format!("BAR {}", bar + 1),
            FontId::proportional(10.0),
            LABEL_DIM,
        );
    }

    let (pattern_bytes, lengths) = match seq_state.patterns.lock() {
        Ok(p) => (p[pattern].clone(), (0..NUM_TRACKS).map(|t| seq::track_length(&p[pattern], t)).collect::<Vec<_>>()),
        Err(_) => return false,
    };
    let running = seq_state.running.load(Ordering::Acquire) && active == pattern;
    let mutes = seq_state.mutes.load(Ordering::Acquire);

    for track in 0..NUM_TRACKS {
        let y = grid.top() + track as f32 * lane_h;
        let lane = Rect::from_min_size(Pos2::new(16.0, y), Vec2::new(w - 32.0, lane_h));
        let selected = st.last_track == track;
        if selected {
            painter.rect_filled(lane, 0.0, Color32::from_rgb(40, 26, 32));
        } else if track % 2 == 1 {
            painter.rect_filled(lane, 0.0, Color32::from_rgb(28, 28, 31));
        }
        // label (click: show this track in the detail strip), mute and length
        let muted = mutes & (1 << track) != 0;
        let name = Rect::from_min_size(Pos2::new(16.0, y), Vec2::new(64.0, lane_h));
        if ui.allocate_rect(name, Sense::click()).on_hover_text("show this track in the detail strip below").clicked() {
            st.last_track = track;
        }
        let color = if selected { LED_RED } else if muted { LABEL_DIM } else { LABEL };
        painter.text(Pos2::new(22.0, y + lane_h / 2.0), Align2::LEFT_CENTER, LANE_NAMES[track], FontId::proportional(12.0), color);
        let mute = Rect::from_min_size(Pos2::new(82.0, y + lane_h / 2.0 - 10.0), Vec2::new(20.0, 20.0));
        if small_button(ui, mute, "M", muted).on_hover_text("mute this track").clicked() {
            seq_state.mutes.fetch_xor(1 << track, Ordering::AcqRel);
        }
        let len_rect = Rect::from_min_size(Pos2::new(106.0, y + lane_h / 2.0 - 10.0), Vec2::new(34.0, 20.0));
        let len_resp = ui.allocate_rect(len_rect, Sense::click_and_drag()).on_hover_text("track length: drag or scroll");
        painter.rect_filled(len_rect, 3.0, Color32::from_rgb(12, 12, 13));
        painter.text(len_rect.center(), Align2::CENTER_CENTER, lengths[track].to_string(), FontId::monospace(12.0), LCD_ON);
        let delta = drag_steps(&len_resp, &mut st.drag_accum, 6.0) + scroll_steps(ui, &len_resp, &mut st.scroll_accum);
        if delta != 0 {
            let old = st.last_track;
            st.last_track = track;
            sequencer::change_track_length(st, track, delta);
            st.last_track = old;
        }

        // steps
        let playhead = seq_state.step[track].load(Ordering::Acquire) as usize;
        for step in 0..STEPS {
            let cell = Rect::from_min_size(Pos2::new(grid.left() + step as f32 * cell_w, y), Vec2::new(cell_w, lane_h)).shrink(1.5);
            let inside = step < lengths[track] as usize;
            let on = seq::step_on(&pattern_bytes, track, step);
            let base = if step % 4 == 0 { Color32::from_rgb(58, 58, 64) } else { Color32::from_rgb(44, 44, 49) };
            let fill = if on {
                // brightness follows the velocity
                let v = seq::step(&pattern_bytes, track, step).velocity as f32 / 127.0;
                let k = 0.35 + 0.65 * v;
                Color32::from_rgb((255.0 * k) as u8, (92.0 * k) as u8, (110.0 * k) as u8)
            } else {
                base
            };
            let fill = if inside { fill } else { fill.gamma_multiply(0.35) };
            painter.rect_filled(cell, 2.0, if muted && on { fill.gamma_multiply(0.5) } else { fill });
            if running && playhead == step {
                painter.rect_stroke(cell.expand(1.0), 2.0, Stroke::new(2.0_f32, Color32::WHITE), StrokeKind::Outside);
            }
            if st.sel_step == Some(step) && st.last_track == track && st.page == Page::Step {
                painter.rect_stroke(cell.expand(1.5), 2.0, Stroke::new(2.0_f32, Color32::from_rgb(255, 200, 60)), StrokeKind::Outside);
            }
        }
        // bar lines
        for bar in 1..4 {
            let x = grid.left() + (bar * 16) as f32 * cell_w;
            painter.line_segment([Pos2::new(x, y), Pos2::new(x, y + lane_h)], Stroke::new(1.5_f32, PANEL));
        }
    }

    // ---- clicking and painting
    let resp = ui.allocate_rect(grid, Sense::click_and_drag());
    let cell_at = |pos: Pos2| -> Option<(usize, usize)> {
        if !grid.contains(pos) {
            return None;
        }
        let step = (((pos.x - grid.left()) / cell_w) as usize).min(STEPS - 1);
        let track = (((pos.y - grid.top()) / lane_h) as usize).min(NUM_TRACKS - 1);
        Some((track, step))
    };
    let pointer = resp.interact_pointer_pos().or_else(|| resp.hover_pos());
    let command = ui.input(|i| i.modifiers.command);
    if resp.secondary_clicked() || (resp.clicked() && command) {
        if let Some((track, step)) = pointer.and_then(cell_at) {
            st.last_track = track;
            st.sel_step = Some(step);
            st.page = Page::Step;
            return true;
        }
    }
    if !command {
        if resp.drag_started() || (resp.is_pointer_button_down_on() && st.lane_paint.is_none() && ui.input(|i| i.pointer.primary_pressed())) {
            // the first cell decides whether the stroke turns steps on or off, and its track is
            // the one the detail strip shows
            if let Some((track, step)) = pointer.and_then(cell_at) {
                st.lane_paint = Some(!seq::step_on(&pattern_bytes, track, step));
                st.last_track = track;
            }
        }
        if let (Some(target), Some((track, step))) = (st.lane_paint, pointer.and_then(cell_at)) {
            if resp.is_pointer_button_down_on() && seq::step_on(&pattern_bytes, track, step) != target {
                if let Ok(mut p) = seq_state.patterns.lock() {
                    seq::toggle_step(&mut p[pattern], track, step);
                }
                seq_state.mark_dirty(pattern);
            }
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            st.lane_paint = None;
        }
    }
    let strip = Rect::from_min_max(Pos2::new(grid.left(), grid.bottom() + 40.0), Pos2::new(grid.right(), h - 40.0));
    if st.maestro_open {
        let area = Rect::from_min_max(Pos2::new(16.0, grid.bottom() + 12.0), Pos2::new(w - 16.0, h - 36.0));
        maestro_panel::draw(ui, st, area);
    } else {
        lane_detail::draw(ui, st, grid.bottom() + 10.0, strip, cell_w);
    }

    let message = st.copy.message.as_ref().filter(|(_, until)| now < *until).map(|(text, _)| text.clone());
    if let Some(text) = message {
        painter.text(Pos2::new(w / 2.0, h - 20.0), Align2::CENTER_CENTER, text, FontId::proportional(12.0), LCD_ON);
    } else if let Some((track, step)) = resp.hover_pos().and_then(cell_at) {
        let d = seq::step(&pattern_bytes, track, step);
        painter.text(
            Pos2::new(w / 2.0, h - 20.0),
            Align2::CENTER_CENTER,
            format!(
                "{} · step {} (bar {}, {}) · vel {} · prob {}% · click: on/off · drag: paint · right-click or ⌘-click: edit",
                LANE_NAMES[track],
                step + 1,
                step / 16 + 1,
                step % 16 + 1,
                d.velocity,
                d.prob
            ),
            FontId::proportional(11.0),
            LABEL_DIM,
        );
    }
    false
}
