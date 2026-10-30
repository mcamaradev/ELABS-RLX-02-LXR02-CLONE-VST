//! ENTROPIA MOD FX view (FX button while the FX mode is ENTROPIA MOD): the per-voice sends to delay, ring
//! and reverb, those three effects, and the master chain filter → drive → comp. Every control is a
//! plugin parameter (entropia.rs): automatable, and saved with the session rather than the kit.
//! Drag or scroll a bar; double-click or right-click puts it back to its default (the filter: off).

use super::*;
use crate::entropia::{self as ent, FxParam, BUSES, VOICES};
use nih_plug_egui::egui::Painter;

const AMBER: Color32 = Color32::from_rgb(255, 150, 70);

fn param(st: &State, id: u16) -> &FxParam {
    &st.params.fx[id as usize].1
}

/// A 0-100 % bar for a float setting; `bipolar` draws it from the centre (the filter).
fn bar(ui: &mut Ui, setter: &ParamSetter, p: &FloatParam, rect: Rect, label: &str, text: String, bipolar: bool) {
    let resp = ui.allocate_rect(rect, Sense::click_and_drag());
    let id = resp.id;
    let down = resp.is_pointer_button_down_on();
    let was_down = ui.memory(|m| m.data.get_temp::<bool>(id)).unwrap_or(false);
    if down && !was_down {
        setter.begin_set_parameter(p);
    }
    if down {
        if let Some(pos) = resp.interact_pointer_pos() {
            setter.set_parameter(p, ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0));
        }
    }
    if !down && was_down {
        setter.end_set_parameter(p);
    }
    ui.memory_mut(|m| m.data.insert_temp(id, down));
    if resp.hovered() && !down {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        if dy != 0.0 {
            setter.begin_set_parameter(p);
            setter.set_parameter(p, (p.value() + dy * 0.002).clamp(0.0, 1.0));
            setter.end_set_parameter(p);
        }
    }
    if resp.secondary_clicked() || resp.double_clicked() {
        setter.begin_set_parameter(p);
        setter.set_parameter(p, p.default_plain_value());
        setter.end_set_parameter(p);
    }

    let painter = ui.painter();
    let v = p.value();
    painter.rect_filled(rect, 3.0, Color32::from_rgb(12, 12, 13));
    let fill = if bipolar {
        let mid = rect.center().x;
        let x = rect.left() + rect.width() * v;
        Rect::from_x_y_ranges(mid.min(x)..=mid.max(x), rect.y_range())
    } else {
        Rect::from_min_size(rect.min, Vec2::new(rect.width() * v, rect.height()))
    };
    let color = if resp.hovered() || down { LED_RED } else { LED_RED.gamma_multiply(0.7) };
    painter.rect_filled(fill, 3.0, color);
    if bipolar {
        painter.line_segment([rect.center_top(), rect.center_bottom()], Stroke::new(1.0_f32, LABEL_DIM));
    }
    if bipolar {
        // the ends say which filter; the value sits in the middle
        painter.text(rect.left_center() + Vec2::new(6.0, 0.0), Align2::LEFT_CENTER, "LP", FontId::proportional(11.0), LABEL);
        painter.text(rect.right_center() - Vec2::new(6.0, 0.0), Align2::RIGHT_CENTER, "HP", FontId::proportional(11.0), LABEL);
        painter.text(rect.center() + Vec2::new(0.0, rect.height() / 2.0 + 9.0), Align2::CENTER_CENTER, text, FontId::monospace(11.0), LABEL);
        return;
    }
    painter.text(rect.left_center() + Vec2::new(6.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(11.0), LABEL);
    painter.text(rect.right_center() - Vec2::new(6.0, 0.0), Align2::RIGHT_CENTER, text, FontId::monospace(11.0), LABEL);
}

fn amount(ui: &mut Ui, setter: &ParamSetter, st: &State, id: u16, rect: Rect, label: &str) {
    if let FxParam::Amount(p) = param(st, id) {
        let text = format!("{:.0}", p.value() * 100.0);
        bar(ui, setter, p, rect, label, text, false);
    }
}

/// A row of buttons for a choice setting.
fn choice(ui: &mut Ui, setter: &ParamSetter, st: &State, id: u16, pos: Pos2, width: f32, names: &[&str]) {
    let FxParam::Choice(p) = param(st, id) else { return };
    let w = (width - 4.0 * (names.len() - 1) as f32) / names.len() as f32;
    for (i, name) in names.iter().enumerate() {
        let rect = Rect::from_min_size(pos + Vec2::new(i as f32 * (w + 4.0), 0.0), Vec2::new(w, 20.0));
        if lanes::small_button(ui, rect, name, p.value() == i as i32).clicked() {
            setter.begin_set_parameter(p);
            setter.set_parameter(p, i as i32);
            setter.end_set_parameter(p);
        }
    }
}

/// The ring LFO's rate: in Hz, or with SYNC on a note length at the host's tempo.
fn lfo_rate(ui: &mut Ui, setter: &ParamSetter, st: &State, rect: Rect) {
    let (FxParam::Amount(rate), FxParam::Choice(sync)) = (param(st, ent::RING_LFO_RATE), param(st, ent::RING_LFO_SYNC)) else { return };
    let button = Rect::from_min_max(Pos2::new(rect.right() - 42.0, rect.top()), rect.max);
    let bar_rect = Rect::from_min_max(rect.min, Pos2::new(button.left() - 4.0, rect.bottom()));
    let synced = sync.value() > 0;
    let help = "SYNC: the LFO runs at the host's tempo in note lengths, and follows the song position while the host plays";
    if lanes::small_button(ui, button, "SYNC", synced).on_hover_text(help).clicked() {
        setter.begin_set_parameter(sync);
        setter.set_parameter(sync, if synced { 0 } else { ent::LFO_SYNC_DEFAULT });
        setter.end_set_parameter(sync);
    }
    if !synced {
        let hz = ent::lfo_hz(rate.value());
        let text = if hz < 10.0 { format!("{hz:.2} Hz") } else { format!("{hz:.1} Hz") };
        bar(ui, setter, rate, bar_rect, "LFO RATE", text, false);
        return;
    }
    // synced: the bar steps through the note lengths, slowest on the left
    let resp = ui.allocate_rect(bar_rect, Sense::click_and_drag());
    let last = ent::LFO_SYNCS.len() as i32 - 1;
    let set = |v: i32| {
        let v = v.clamp(1, last);
        if v != sync.value() {
            setter.begin_set_parameter(sync);
            setter.set_parameter(sync, v);
            setter.end_set_parameter(sync);
        }
    };
    if resp.is_pointer_button_down_on() {
        if let Some(pos) = resp.interact_pointer_pos() {
            set(1 + ((pos.x - bar_rect.left()) / bar_rect.width() * last as f32).floor() as i32);
        }
    }
    if resp.hovered() {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        if dy.abs() > 0.5 {
            set(sync.value() + dy.signum() as i32);
        }
    }
    if resp.double_clicked() || resp.secondary_clicked() {
        set(ent::LFO_SYNC_DEFAULT);
    }
    let painter = ui.painter();
    painter.rect_filled(bar_rect, 3.0, Color32::from_rgb(12, 12, 13));
    let fill = sync.value() as f32 / last as f32;
    let color = if resp.hovered() { LED_RED } else { LED_RED.gamma_multiply(0.7) };
    painter.rect_filled(Rect::from_min_size(bar_rect.min, Vec2::new(bar_rect.width() * fill, bar_rect.height())), 3.0, color);
    painter.text(bar_rect.left_center() + Vec2::new(6.0, 0.0), Align2::LEFT_CENTER, "LFO RATE", FontId::proportional(11.0), LABEL);
    let name = ent::LFO_SYNCS[sync.value().clamp(0, last) as usize];
    painter.text(bar_rect.right_center() - Vec2::new(6.0, 0.0), Align2::RIGHT_CENTER, name, FontId::monospace(11.0), LABEL);
}

fn section(painter: &Painter, rect: Rect, title: &str, help: &str, ui: &mut Ui) {
    painter.rect_stroke(rect, 6.0, Stroke::new(1.0_f32, PANEL_EDGE), StrokeKind::Inside);
    let title_rect = Rect::from_min_size(rect.min + Vec2::new(10.0, 6.0), Vec2::new(120.0, 16.0));
    painter.text(title_rect.left_center(), Align2::LEFT_CENTER, title, FontId::proportional(13.0), AMBER);
    ui.allocate_rect(title_rect, Sense::hover()).on_hover_text(help);
}

/// Returns true when the view should close.
pub(super) fn draw(ui: &mut Ui, setter: &ParamSetter, st: &mut State) -> bool {
    let (w, h) = (WIDTH as f32, HEIGHT as f32);
    let painter = ui.painter().clone();
    painter.rect_stroke(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h)).shrink(4.0), 10.0, Stroke::new(2.0_f32, PANEL_EDGE), StrokeKind::Inside);
    let params = st.params.clone();

    // ---- header
    if lanes::small_button(ui, Rect::from_min_size(Pos2::new(16.0, 14.0), Vec2::new(92.0, 26.0)), "◀ PANEL", false).clicked() {
        return true;
    }
    painter.text(Pos2::new(124.0, 27.0), Align2::LEFT_CENTER, "ENTROPIA MOD", FontId::proportional(17.0), LED_RED);
    painter.text(
        Pos2::new(250.0, 27.0),
        Align2::LEFT_CENTER,
        "every voice to delay, ring and reverb at once · master filter > drive > comp · double-click: default",
        FontId::proportional(11.0),
        LABEL_DIM,
    );
    let entropia = params.fx_mode.value();
    for (i, name) in ent::FX_MODES.iter().enumerate() {
        let rect = Rect::from_min_size(Pos2::new(w - 228.0 + i as f32 * 108.0, 14.0), Vec2::new(104.0, 26.0));
        let help = if i == 0 { "the LXR's own FX: one effect, set by the kit (FX page)" } else { "sends and master chain, this page" };
        if lanes::small_button(ui, rect, name, entropia == (i == 1)).on_hover_text(help).clicked() {
            setter.begin_set_parameter(&params.fx_mode);
            setter.set_parameter(&params.fx_mode, i == 1);
            setter.end_set_parameter(&params.fx_mode);
        }
    }
    if !entropia {
        painter.text(
            Pos2::new(w / 2.0, h - 18.0),
            Align2::CENTER_CENTER,
            "CLASSIC is on: these settings wait until you switch to ENTROPIA MOD",
            FontId::proportional(12.0),
            LCD_ON,
        );
    }

    // ---- sends: voice × effect
    let top = 54.0;
    let sends = Rect::from_min_size(Pos2::new(16.0, top), Vec2::new(392.0, 222.0));
    section(&painter, sends, "SENDS", "how much of each voice goes to each effect (after its fader and pan)", ui);
    let col_w = 100.0;
    for (b, bus) in BUSES.iter().enumerate() {
        painter.text(Pos2::new(sends.left() + 92.0 + b as f32 * (col_w + 4.0) + col_w / 2.0, top + 34.0), Align2::CENTER_CENTER, bus.to_uppercase(), FontId::proportional(11.0), LABEL_DIM);
    }
    for (v, voice) in VOICES.iter().enumerate() {
        let y = top + 46.0 + v as f32 * 28.0;
        painter.text(Pos2::new(sends.left() + 12.0, y + 11.0), Align2::LEFT_CENTER, voice, FontId::proportional(12.0), LABEL);
        for b in 0..BUSES.len() {
            let rect = Rect::from_min_size(Pos2::new(sends.left() + 92.0 + b as f32 * (col_w + 4.0), y), Vec2::new(col_w, 22.0));
            amount(ui, setter, st, ent::send_id(b, v), rect, "");
        }
    }

    // ---- the three send effects
    let fx_w = 184.0;
    let row_h = 26.0;
    let boxes = [
        ("DELAY", "the LXR's delay; ping-pong bounces left and right"),
        ("RING", "the LXR's ring modulator; the LFO sweeps FREQ up to 3 octaves either way"),
        ("REVERB", "the reverb in the LXR's firmware, switched off on the hardware"),
    ];
    for (k, (title, help)) in boxes.iter().enumerate() {
        let rect = Rect::from_min_size(Pos2::new(416.0 + k as f32 * (fx_w + 6.0), top), Vec2::new(fx_w, 222.0));
        section(&painter, rect, title, help, ui);
        let x = rect.left() + 10.0;
        let bw = fx_w - 20.0;
        let mut y = top + 30.0;
        let row = |ui: &mut Ui, id: u16, label: &str, y: &mut f32| {
            amount(ui, setter, st, id, Rect::from_min_size(Pos2::new(x, *y), Vec2::new(bw, 22.0)), label);
            *y += row_h;
        };
        match k {
            0 => {
                choice(ui, setter, st, ent::DELAY_TYPE, Pos2::new(x, y), bw, &ent::DELAY_TYPES);
                y += row_h;
                for (id, label) in [(ent::DELAY_TIME, "TIME"), (ent::DELAY_RANGE, "RANGE"), (ent::DELAY_TONE, "TONE"), (ent::DELAY_FEEDBACK, "FEEDBACK")] {
                    row(ui, id, label, &mut y);
                }
                row(ui, ent::DELAY_RETURN, "RETURN", &mut y);
            }
            1 => {
                choice(ui, setter, st, ent::RING_WAVE, Pos2::new(x, y), bw, &ent::RING_WAVES);
                y += row_h;
                row(ui, ent::RING_FREQ, "FREQ", &mut y);
                // the LFO on FREQ
                choice(ui, setter, st, ent::RING_LFO_WAVE, Pos2::new(x, y), bw, &["sin", "tri", "saw", "sqr", "rnd"]);
                y += row_h;
                lfo_rate(ui, setter, st, Rect::from_min_size(Pos2::new(x, y), Vec2::new(bw, 22.0)));
                y += row_h;
                row(ui, ent::RING_LFO_DEPTH, "LFO DEPTH", &mut y);
                row(ui, ent::RING_RETURN, "RETURN", &mut y);
            }
            _ => {
                y += row_h;
                for (id, label) in [(ent::VERB_SIZE, "SIZE"), (ent::VERB_DECAY, "DECAY"), (ent::VERB_TONE, "TONE"), (ent::VERB_VARIATION, "VARIATION")] {
                    row(ui, id, label, &mut y);
                }
                row(ui, ent::VERB_RETURN, "RETURN", &mut y);
            }
        }
    }

    // ---- master chain
    let top = 288.0;
    let chain_h = 182.0;
    let widths = [290.0, 328.0, 328.0];
    let mut x = 16.0;
    for (k, &bw) in widths.iter().enumerate() {
        let rect = Rect::from_min_size(Pos2::new(x, top), Vec2::new(bw, chain_h));
        let (title, help) = match k {
            0 => ("MASTER FILTER", "left of centre a low pass closes, right of centre a high pass opens; centre = off"),
            1 => ("DRIVE", "the LXR's master distortion on everything; MIX 0 = off"),
            _ => ("COMP", "the LXR's compressor on everything; MIX 0 = off"),
        };
        section(&painter, rect, title, help, ui);
        if k > 0 {
            painter.text(Pos2::new(x - 4.0, top + chain_h / 2.0), Align2::CENTER_CENTER, "▶", FontId::proportional(12.0), AMBER);
        }
        let bx = rect.left() + 10.0;
        let bar_w = bw - 20.0;
        let mut y = top + 30.0;
        let at = |y: f32| Rect::from_min_size(Pos2::new(bx, y), Vec2::new(bar_w, 22.0));
        match k {
            0 => {
                if let FxParam::Amount(p) = param(st, ent::FILTER) {
                    let text = ent::filter_text(p.value());
                    bar(ui, setter, p, Rect::from_min_size(Pos2::new(bx, y), Vec2::new(bar_w, 26.0)), "", text, true);
                }
                y += 52.0;
                amount(ui, setter, st, ent::FILTER_RESO, at(y), "RESONANCE");
                painter.text(
                    Pos2::new(bx, y + 40.0),
                    Align2::LEFT_TOP,
                    "centre: off\nleft: low pass, closing towards 50 Hz\nright: high pass, opening up to 8 kHz",
                    FontId::proportional(11.0),
                    LABEL_DIM,
                );
            }
            1 => {
                choice(ui, setter, st, ent::DRIVE_TYPE, Pos2::new(bx, y), bar_w, &ent::DRIVE_TYPES);
                y += row_h;
                for (id, label) in [(ent::DRIVE, "DRIVE"), (ent::DRIVE_COLOR, "COLOR"), (ent::DRIVE_TONE, "TONE"), (ent::DRIVE_LEVEL, "LEVEL"), (ent::DRIVE_MIX, "MIX")] {
                    amount(ui, setter, st, id, at(y), label);
                    y += row_h;
                }
            }
            _ => {
                choice(ui, setter, st, ent::COMP_RATIO, Pos2::new(bx, y), bar_w, &ent::COMP_RATIOS);
                y += row_h;
                for (id, label) in [(ent::COMP_ATTACK, "ATTACK"), (ent::COMP_RELEASE, "RELEASE"), (ent::COMP_THRESH, "THRESHOLD"), (ent::COMP_GAIN, "GAIN"), (ent::COMP_MIX, "MIX")] {
                    amount(ui, setter, st, id, at(y), label);
                    y += row_h;
                }
            }
        }
        x += bw + 8.0;
    }
    if entropia {
        painter.text(Pos2::new(w / 2.0, h - 18.0), Align2::CENTER_CENTER, "ENTROPIA MOD mixes both outputs into OUT 1", FontId::proportional(11.0), LABEL_DIM);
    }
    false
}
