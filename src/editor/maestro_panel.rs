//! The Maestro's panel: behind the little robot in the LANES header, in place of the detail strip.
//! See maestro.rs for what it plays. Left: the groove (variation, fills, tracks); right: the moves,
//! each with its switch, CHANCE and DEPTH.

use super::*;
use crate::maestro::{fills_for, FILL_NAMES, MOVES, MOVE_HELP, PHRASES, SENDS};
use nih_plug_egui::egui::Painter;

/// The robot: a head with an antenna; its eyes light up while the Maestro plays.
pub(super) fn robot(painter: &Painter, center: Pos2, size: f32, color: Color32, eyes: Color32) {
    let head = Rect::from_center_size(center + Vec2::new(0.0, size * 0.1), Vec2::new(size * 0.8, size * 0.6));
    painter.rect_stroke(head, size * 0.12, Stroke::new(size * 0.08, color), StrokeKind::Middle);
    let top = Pos2::new(center.x, head.top());
    painter.line_segment([top, top - Vec2::new(0.0, size * 0.18)], Stroke::new(size * 0.07, color));
    painter.circle_filled(top - Vec2::new(0.0, size * 0.22), size * 0.07, color);
    for dx in [-0.17, 0.17] {
        painter.circle_filled(head.center() + Vec2::new(dx * size, -size * 0.04), size * 0.08, eyes);
    }
    painter.line_segment(
        [head.center() + Vec2::new(-size * 0.14, size * 0.14), head.center() + Vec2::new(size * 0.14, size * 0.14)],
        Stroke::new(size * 0.05, color),
    );
}

/// A 0-100 bar: click or drag to set; `dim` when what it sets does nothing right now.
fn amount(ui: &mut Ui, rect: Rect, value: u8, help: &str, dim: bool) -> Option<u8> {
    let resp = ui.allocate_rect(rect, Sense::click_and_drag()).on_hover_text(help);
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, Color32::from_rgb(12, 12, 13));
    let filled = Rect::from_min_size(rect.min, Vec2::new(rect.width() * value as f32 / 100.0, rect.height()));
    painter.rect_filled(filled, 3.0, if dim { BUTTON_DIM } else { LED_RED.gamma_multiply(0.75) });
    painter.text(rect.center(), Align2::CENTER_CENTER, format!("{value} %"), FontId::monospace(12.0), if dim { LABEL_DIM } else { LABEL });
    let pos = resp.interact_pointer_pos().filter(|_| resp.is_pointer_button_down_on() || resp.clicked())?;
    Some(((pos.x - rect.left()) / rect.width() * 100.0).round().clamp(0.0, 100.0) as u8)
}

pub(super) fn draw(ui: &mut Ui, st: &mut State, area: Rect) {
    let painter = ui.painter().clone();
    let params = st.params.clone();
    let m = &params.maestro;
    let on = m.on.load(Ordering::Acquire);
    let rec = params.seq.record.load(Ordering::Acquire);
    let running = params.seq.running.load(Ordering::Acquire);
    let (left, top) = (area.left(), area.top());
    let label = |x: f32, y: f32, text: &str| {
        painter.text(Pos2::new(x, y), Align2::LEFT_CENTER, text, FontId::proportional(11.0), LABEL_DIM);
    };

    // ---- title, on/off and what it is doing
    robot(&painter, Pos2::new(left + 14.0, top + 13.0), 26.0, LABEL, if on { LED_RED } else { BUTTON_DIM });
    painter.text(Pos2::new(left + 36.0, top + 13.0), Align2::LEFT_CENTER, "MAESTRO", FontId::proportional(15.0), if on { LED_RED } else { LABEL });
    let toggle = Rect::from_min_size(Pos2::new(left + 130.0, top + 1.0), Vec2::new(64.0, 24.0));
    if lanes::small_button(ui, toggle, if on { "ON" } else { "OFF" }, on)
        .on_hover_text("plays your pattern a little differently every bar, with a fill at the end of each phrase; your pattern itself never changes")
        .clicked()
    {
        m.on.store(!on, Ordering::Release);
    }
    let phrase = m.phrase.load(Ordering::Acquire);
    let status = match (on, rec, running) {
        (false, _, _) => "off: the sequencer plays your pattern as written".to_string(),
        (true, true, _) => "paused while REC is on".to_string(),
        (true, false, false) => "waiting for the host to play".to_string(),
        (true, false, true) => {
            let last = match m.last_fill.load(Ordering::Acquire) {
                0 => "none yet".to_string(),
                f => FILL_NAMES[(f - 1) as usize % FILL_NAMES.len()].to_string(),
            };
            match phrase {
                0 => format!("playing · last fill: {last}"),
                n => format!("bar {} of {n} · last fill: {last}", m.bar.load(Ordering::Acquire) as u32 + 1),
            }
        }
    };
    painter.text(Pos2::new(left + 210.0, top + 13.0), Align2::LEFT_CENTER, status, FontId::proportional(12.0), LCD_ON);
    // the phrase's bars, the fill bar last
    if on && running && !rec && phrase > 0 {
        let bar = m.bar.load(Ordering::Acquire);
        let w = 14.0;
        for b in 0..phrase {
            let r = Rect::from_min_size(Pos2::new(area.right() - (phrase - b) as f32 * (w + 3.0), top + 6.0), Vec2::new(w, 14.0));
            let color = match (b == bar, b == phrase - 1) {
                (true, _) => LED_RED,
                (false, true) => Color32::from_rgb(90, 40, 56),
                _ => BUTTON_DIM,
            };
            painter.rect_filled(r, 2.0, color);
        }
    }

    // ---- left: the groove
    let row = top + 40.0;
    label(left, row + 11.0, "VARIATION");
    let variation = m.variation.load(Ordering::Acquire);
    let help = "how much each bar differs: velocities, ghost notes, hat ratchets, percs moving a step (the kick only breathes)";
    if let Some(v) = amount(ui, Rect::from_min_size(Pos2::new(left + 76.0, row), Vec2::new(250.0, 22.0)), variation, help, false) {
        m.variation.store(v, Ordering::Release);
    }
    let fill_rect = Rect::from_min_size(Pos2::new(left + 336.0, row), Vec2::new(100.0, 22.0));
    let waiting = m.fill_now.load(Ordering::Acquire);
    if lanes::small_button(ui, fill_rect, "FILL NOW", waiting).on_hover_text("a fill on the next third beat").clicked() && on {
        m.fill_now.store(true, Ordering::Release);
    }

    let row = top + 72.0;
    label(left, row + 11.0, "FILL EVERY");
    for (i, &bars) in PHRASES.iter().enumerate() {
        let rect = Rect::from_min_size(Pos2::new(left + 76.0 + i as f32 * 56.0, row), Vec2::new(52.0, 22.0));
        let text = if bars == 0 { "never".to_string() } else { format!("{bars} bars") };
        if lanes::small_button(ui, rect, &text, phrase == bars).on_hover_text("a fill on the last beat of every phrase this long").clicked() {
            m.phrase.store(bars, Ordering::Release);
        }
    }

    let row = top + 104.0;
    label(left, row + 11.0, "TRACKS");
    let tracks = m.tracks.load(Ordering::Acquire);
    for (t, name) in lanes::LANE_NAMES.iter().enumerate() {
        let rect = Rect::from_min_size(Pos2::new(left + 76.0 + t as f32 * 52.0, row), Vec2::new(49.0, 22.0));
        let lit = tracks & (1 << t) != 0;
        if lanes::small_button(ui, rect, name, lit).on_hover_text("let the Maestro play with this track").clicked() {
            m.tracks.store(tracks ^ (1 << t), Ordering::Release);
        }
    }
    let names: Vec<&str> = fills_for(tracks).iter().map(|&f| FILL_NAMES[f]).collect();
    let fills = if names.is_empty() { "no fill fits these tracks".to_string() } else { format!("fills: {}", names.join(" · ")) };
    label(left, top + 142.0, &fills);
    label(left, top + 162.0, "your pattern and settings stay as you wrote them");

    // ---- right: the moves
    let x0 = left + 486.0;
    let (bar_w, name_w) = (150.0, 96.0);
    let entropia = params.fx_mode.value();
    painter.text(Pos2::new(x0 + name_w + 4.0 + bar_w / 2.0, top + 38.0), Align2::CENTER_CENTER, "CHANCE", FontId::proportional(10.5), LABEL_DIM);
    painter.text(Pos2::new(x0 + name_w + 8.0 + bar_w * 1.5, top + 38.0), Align2::CENTER_CENTER, "DEPTH", FontId::proportional(10.5), LABEL_DIM);
    for (i, (name, help)) in MOVES.iter().zip(MOVE_HELP).enumerate() {
        let mv = &m.moves[i];
        let y = top + 48.0 + i as f32 * 25.0;
        // the last three only act in ENTROPIA MOD
        let idle = i >= SENDS && !entropia;
        let move_on = mv.on.load(Ordering::Acquire);
        let help = if idle { format!("{help} (switch the FX mode to ENTROPIA MOD)") } else { help.to_string() };
        let rect = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(name_w, 22.0));
        if lanes::small_button(ui, rect, name, move_on && !idle).on_hover_text(&help).clicked() {
            mv.on.store(!move_on, Ordering::Release);
        }
        let dim = !move_on || idle;
        let chance_help = if i == crate::maestro::FILTER { "how many fills get a sweep (other bars: now and then a short one)" } else { "how many bars it moves in" };
        let chance = mv.chance.load(Ordering::Acquire);
        if let Some(v) = amount(ui, Rect::from_min_size(Pos2::new(x0 + name_w + 4.0, y), Vec2::new(bar_w, 22.0)), chance, chance_help, dim) {
            mv.chance.store(v, Ordering::Release);
        }
        let depth = mv.depth.load(Ordering::Acquire);
        if let Some(v) = amount(ui, Rect::from_min_size(Pos2::new(x0 + name_w + 8.0 + bar_w, y), Vec2::new(bar_w, 22.0)), depth, "how far it goes", dim) {
            mv.depth.store(v, Ordering::Release);
        }
    }
    ui.ctx().request_repaint();
}
