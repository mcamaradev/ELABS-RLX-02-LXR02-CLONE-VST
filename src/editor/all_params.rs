//! "ALL PARAMS" view: every parameter of one voice at once, in the order of the LXR's menu pages
//! (OSC, AEG, MOD, FM, CLICK, FILTER, LFO, MIX). Touching a control here while Live's Configure is
//! on adds it to the device panel, where it can be MIDI mapped. The FX tab follows the FX MODE:
//! the LXR's FX pages (CLASSIC) or the ENTROPIA MOD settings, with the switch on top.

use super::*;
use crate::entropia::{self as ent, FxParam};
use crate::param_table::PARAMS;

const TABS: [&str; 7] = ["DRUM 1", "DRUM 2", "DRUM 3", "SNARE", "CYMBAL", "HIHAT", "FX / MASTER"];
const ROW_LABELS: [&str; 8] = ["OSC", "AEG", "MOD", "FM", "CLICK", "FILTER", "LFO", "MIX"];
const FX_ROW_LABELS: [&str; 6] = ["FX", "DRIVE", "RING", "COMP", "DELAY", "REVERB"];
const COLUMNS: usize = 8;

enum Control<'a> {
    Int(&'a IntParam, u16),
    Float(&'a FloatParam),
}

/// Rows of (label, controls) for a tab. Parameters already shown in an earlier row are skipped.
fn rows<'a>(params: &'a LxrParams, tab: usize) -> Vec<(&'static str, Vec<Control<'a>>)> {
    let mut seen = std::collections::HashSet::new();
    let mut rows = Vec::new();
    let mut push_page = |label: &'static str, page: &SubPage, rows: &mut Vec<(&'static str, Vec<Control<'a>>)>| {
        let controls: Vec<Control<'a>> = page
            .iter()
            .flatten()
            .filter(|s| seen.insert(s.param))
            .filter_map(|s| params.by_index(s.param).map(|p| Control::Int(p, s.param)))
            .collect();
        if let Some(row) = rows.iter_mut().find(|(l, _)| *l == label) {
            row.1.extend(controls);
        } else if !controls.is_empty() {
            rows.push((label, controls));
        }
    };
    match tab {
        0..=5 => {
            // the hihat has two menu pages (closed and open)
            let pages: &[usize] = if tab == 5 { &[5, 6] } else { &[tab] };
            for &v in pages {
                for (sub, page) in VOICE_PAGES[v].iter().enumerate() {
                    push_page(ROW_LABELS[sub], page, &mut rows);
                }
            }
            if let Some(row) = rows.iter_mut().find(|(l, _)| *l == "MIX") {
                row.1.push(Control::Float(&params.faders[tab]));
            }
        }
        _ if params.fx_mode.value() => {
            let control = |id: u16| match &params.fx[id as usize].1 {
                FxParam::Amount(p) => Control::Float(p),
                FxParam::Choice(p) => Control::Int(p, u16::MAX),
            };
            let group = |ids: &[u16]| ids.iter().map(|&id| control(id)).collect::<Vec<_>>();
            for (bus, label) in ["DLY SEND", "RING SEND", "REV SEND"].iter().enumerate() {
                rows.push((*label, (0..ent::VOICES.len()).map(|v| control(ent::send_id(bus, v))).collect()));
            }
            rows.push(("DELAY", group(&[ent::DELAY_TYPE, ent::DELAY_TIME, ent::DELAY_RANGE, ent::DELAY_TONE, ent::DELAY_FEEDBACK, ent::DELAY_RETURN])));
            rows.push(("RING", group(&[ent::RING_WAVE, ent::RING_FREQ, ent::RING_LFO_WAVE, ent::RING_LFO_RATE, ent::RING_LFO_DEPTH, ent::RING_LFO_SYNC, ent::RING_RETURN])));
            rows.push(("REVERB", group(&[ent::VERB_SIZE, ent::VERB_DECAY, ent::VERB_TONE, ent::VERB_VARIATION, ent::VERB_RETURN])));
            rows.push(("FILTER", group(&[ent::FILTER, ent::FILTER_RESO])));
            rows.push(("DRIVE", group(&[ent::DRIVE_TYPE, ent::DRIVE, ent::DRIVE_COLOR, ent::DRIVE_TONE, ent::DRIVE_LEVEL, ent::DRIVE_MIX])));
            rows.push(("COMP", group(&[ent::COMP_RATIO, ent::COMP_ATTACK, ent::COMP_RELEASE, ent::COMP_THRESH, ent::COMP_GAIN, ent::COMP_MIX])));
            rows.push(("MASTER", master_row(params)));
        }
        _ => {
            for (i, page) in FX_PAGES.iter().enumerate() {
                push_page(FX_ROW_LABELS[i], page, &mut rows);
            }
            rows.push(("MASTER", master_row(params)));
        }
    }
    // rows with more controls than columns wrap onto continuation rows
    rows.into_iter()
        .flat_map(|(label, controls)| {
            let mut out = Vec::new();
            let mut it = controls.into_iter().peekable();
            let mut first = true;
            while it.peek().is_some() {
                out.push((if first { label } else { "" }, it.by_ref().take(COLUMNS).collect()));
                first = false;
            }
            out
        })
        .collect()
}

fn master_row(params: &LxrParams) -> Vec<Control<'_>> {
    PARAMS
        .iter()
        .filter(|info| info.group == "Master")
        .filter_map(|info| params.by_index(info.index).map(|p| Control::Int(p, info.index)))
        .chain(std::iter::once(Control::Int(&params.morph, u16::MAX)))
        .chain(std::iter::once(Control::Float(&params.volume)))
        .collect()
}

/// The parameter's name without its voice prefix and shortened to fit a cell
/// ("Drum 1 Velocity Mod Amount" -> "Velo Amount").
fn short_name(name: &str) -> String {
    let mut n = name;
    // (ENTROPIA MOD: the row says which effect)
    let prefixes = [
        "Drum 1 ", "Drum 2 ", "Drum 3 ", "Snare ", "Cymbal ", "Hihat ", "Master ", "FX ", "Delay Send ", "Ring Send ", "Reverb Send ", "Delay ",
        "Ring ", "Reverb ", "Drive ", "Comp ",
    ];
    for prefix in prefixes {
        if let Some(rest) = n.strip_prefix(prefix) {
            n = rest;
            break;
        }
    }
    match n {
        "Velocity Dest" => "Velo Dest".into(),
        "Velocity Mod Amount" => "Velo Amount".into(),
        "Velocity to Volume" => "Velo > Vol".into(),
        "LFO Dest Voice" => "LFO Voice".into(),
        "LFO Retrigger" => "LFO Retrig".into(),
        _ => n.replace("Transient ", "Click ").replace("Filter ", "Filt ").replace("Mod Osc ", "Osc "),
    }
}

pub(super) fn draw(ui: &mut Ui, setter: &ParamSetter, st: &mut State, tab: usize) {
    let painter = ui.painter().clone();
    let full = Rect::from_min_size(Pos2::ZERO, Vec2::new(WIDTH as f32, HEIGHT as f32));
    painter.rect_stroke(full.shrink(4.0), 10.0, Stroke::new(2.0_f32, PANEL_EDGE), StrokeKind::Inside);

    // back to the panel, then one tab per voice
    let back = Rect::from_min_size(Pos2::new(16.0, 14.0), Vec2::new(92.0, 26.0));
    let resp = ui.allocate_rect(back, Sense::click());
    painter.rect_filled(back, 4.0, if resp.hovered() { BUTTON } else { BUTTON_DIM });
    painter.text(back.center(), Align2::CENTER_CENTER, "◀ PANEL", FontId::proportional(13.0), LABEL);
    if resp.clicked() {
        st.all_tab = None;
        return;
    }
    let tab_w = (WIDTH as f32 - 140.0) / TABS.len() as f32;
    for (i, name) in TABS.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(124.0 + i as f32 * tab_w, 14.0), Vec2::new(tab_w - 6.0, 26.0));
        let resp = ui.allocate_rect(r, Sense::click());
        let lit = i == tab;
        if lit {
            painter.rect_filled(r.expand(2.0), 5.0, Color32::from_rgba_unmultiplied(255, 66, 130, 40));
        }
        painter.rect_filled(r, 4.0, if lit { LED_RED } else if resp.hovered() { BUTTON } else { BUTTON_DIM });
        painter.text(r.center(), Align2::CENTER_CENTER, *name, FontId::proportional(12.0), if lit { PANEL } else { LABEL });
        if resp.clicked() {
            st.all_tab = Some(i);
        }
    }

    let params = st.params.clone();
    let mut top = 54.0;
    if tab == TABS.len() - 1 {
        fx_mode_row(ui, setter, &params, top);
        top += 34.0;
    }
    let rows = rows(&params, tab);
    let row_h = ((HEIGHT as f32 - top - 28.0) / rows.len().max(8) as f32).min(54.0);
    let label_w = 70.0;
    let cell_w = (WIDTH as f32 - label_w - 24.0) / COLUMNS as f32;
    let mut hovered_text = None;
    for (r, (label, controls)) in rows.iter().enumerate() {
        let y = top + r as f32 * row_h;
        if !label.is_empty() {
            painter.line_segment([Pos2::new(16.0, y), Pos2::new(WIDTH as f32 - 16.0, y)], Stroke::new(1.0_f32, PANEL_EDGE));
            painter.text(Pos2::new(20.0, y + row_h / 2.0), Align2::LEFT_CENTER, *label, FontId::proportional(13.0), LED_RED);
        }
        for (c, control) in controls.iter().enumerate() {
            let center = Pos2::new(label_w + 16.0 + c as f32 * cell_w + 16.0, y + row_h / 2.0);
            let radius = (row_h * 0.32).min(16.0);
            let hit = Rect::from_center_size(center, Vec2::splat(radius * 2.2));
            let resp = ui.allocate_rect(hit, Sense::click_and_drag());
            let (name, value, normalized) = match control {
                Control::Int(param, index) => {
                    let choices = mod_target_choices(&params, *index);
                    let before = param.value();
                    edit_param_drag(ui, &resp, setter, param, choices.as_deref(), &mut st.drag_accum, &mut st.scroll_accum, 3.0);
                    if (PAR_VOICE_LFO1..PAR_VOICE_LFO1 + 6).contains(index) && param.value() != before {
                        follow_lfo_voice(setter, &params, *index, before, param.value());
                    }
                    (short_name(param.name()), param.to_string(), param.unmodulated_normalized_value())
                }
                Control::Float(param) => {
                    edit_float_drag(ui, &resp, setter, param, &mut st.scroll_accum);
                    (short_name(param.name()), param.to_string(), param.unmodulated_normalized_value())
                }
            };
            knob_face(&painter, center, radius, Some(normalized), false);
            let text_x = center.x + radius + 6.0;
            let max_chars = ((cell_w - radius * 2.0 - 10.0) / 5.6) as usize;
            let clip = |s: &str| -> String { s.chars().take(max_chars.max(4)).collect() };
            painter.text(Pos2::new(text_x, center.y - 7.0), Align2::LEFT_CENTER, clip(&name), FontId::proportional(10.5), LABEL);
            painter.text(Pos2::new(text_x, center.y + 8.0), Align2::LEFT_CENTER, clip(&value), FontId::monospace(11.0), LCD_ON);
            if resp.hovered() || resp.dragged() {
                hovered_text = Some(format!("{}: {}", name, value));
            }
        }
    }
    let hint = hovered_text.unwrap_or_else(|| {
        "drag or scroll a knob, double-click resets · with Live's Configure on, touched controls join the device panel for MIDI mapping".to_string()
    });
    painter.text(Pos2::new(WIDTH as f32 / 2.0, HEIGHT as f32 - 16.0), Align2::CENTER_CENTER, hint, FontId::proportional(11.0), LABEL_DIM);
}

/// The FX tab's FX MODE switch; the rows below follow it.
fn fx_mode_row(ui: &mut Ui, setter: &ParamSetter, params: &LxrParams, top: f32) {
    let painter = ui.painter().clone();
    painter.text(Pos2::new(20.0, top + 13.0), Align2::LEFT_CENTER, "FX MODE", FontId::proportional(13.0), LED_RED);
    let entropia = params.fx_mode.value();
    let help = [
        "the LXR's own FX: one effect at a time, set by the kit",
        "every voice to delay, ring and reverb at once, then a master filter, drive and comp",
    ];
    for (i, name) in ent::FX_MODES.iter().enumerate() {
        let rect = Rect::from_min_size(Pos2::new(102.0 + i as f32 * 128.0, top), Vec2::new(122.0, 26.0));
        if lanes::small_button(ui, rect, name, entropia == (i == 1)).on_hover_text(help[i]).clicked() {
            setter.begin_set_parameter(&params.fx_mode);
            setter.set_parameter(&params.fx_mode, i == 1);
            setter.end_set_parameter(&params.fx_mode);
        }
    }
    let note = if entropia { "sends, effects and master chain (also on the FX page)" } else { "the kit's FX bus, as on the LXR (the display on the FX page)" };
    painter.text(Pos2::new(370.0, top + 13.0), Align2::LEFT_CENTER, note, FontId::proportional(11.0), LABEL_DIM);
}

fn edit_float_drag(ui: &Ui, resp: &Response, setter: &ParamSetter, param: &FloatParam, scroll_accum: &mut f32) {
    if resp.drag_started() {
        setter.begin_set_parameter(param);
    }
    if resp.dragged() {
        let v = (param.unmodulated_normalized_value() - resp.drag_delta().y / 200.0).clamp(0.0, 1.0);
        setter.set_parameter_normalized(param, v);
    }
    if resp.drag_stopped() {
        setter.end_set_parameter(param);
    }
    if resp.double_clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, param.default_plain_value());
        setter.end_set_parameter(param);
    }
    let steps = scroll_steps(ui, resp, scroll_accum);
    if steps != 0 {
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, (param.unmodulated_normalized_value() + steps as f32 * 0.02).clamp(0.0, 1.0));
        setter.end_set_parameter(param);
    }
}
