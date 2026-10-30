//! ENTROPIA MOD FX: the plugin's second FX mode, a modification of the LXR's FX (engine/lxr_entropia.c). Every voice sends to a delay, a
//! ring modulator and a reverb, and the mix goes through a master filter, drive and compressor,
//! all at once. CLASSIC is the LXR's own FX bus, one effect at a time, set by the kit.
//!
//! The settings are plugin parameters (automatable, saved with the session), not kit parameters:
//! kits stay as the hardware reads them.

use nih_plug::prelude::*;

/// The FX MODE switch: the LXR's own FX bus, or this.
pub const FX_MODES: [&str; 2] = ["CLASSIC", "ENTROPIA MOD"];
pub const VOICES: [&str; 6] = ["DRUM1", "DRUM2", "DRUM3", "SNARE", "CLP/CYM", "HIHAT"];
pub const BUSES: [&str; 3] = ["Delay", "Ring", "Reverb"];
pub const SEND: u16 = 0;
pub const DELAY_TYPES: [&str; 2] = ["normal", "ping-pong"];
pub const RING_WAVES: [&str; 4] = ["sine", "tri", "saw", "rect"];
pub const DRIVE_TYPES: [&str; 3] = ["tube", "fold", "rat"];
pub const COMP_RATIOS: [&str; 6] = ["1:1", "2:1", "3:1", "4:1", "6:1", "8:1"];
pub const LFO_WAVES: [&str; 5] = ["sine", "tri", "saw", "square", "random"];
/// The ring LFO's SYNC setting: free (RATE in Hz) or a note length at the host's tempo
/// (lxr_entropia.c lfo_beats), slowest first.
pub const LFO_SYNCS: [&str; 16] = [
    "free", "4 bars", "2 bars", "1 bar", "1/2 .", "1/2", "1/4 .", "1/2 T", "1/4", "1/8 .", "1/4 T", "1/8", "1/8 T", "1/16", "1/16 T", "1/32",
];
/// SYNC when the sync button is switched on
pub const LFO_SYNC_DEFAULT: i32 = 8;

pub enum Kind {
    /// 0..1 with its default
    Amount(f32),
    /// names, default
    Choice(&'static [&'static str], i32),
}

/// One setting: lxr_bridge.h LXR_FX_* number, the id the host saves it under, its name.
pub struct Info {
    pub id: u16,
    pub key: &'static str,
    pub name: &'static str,
    pub kind: Kind,
}

const fn amount(id: u16, key: &'static str, name: &'static str, default: f32) -> Info {
    Info { id, key, name, kind: Kind::Amount(default) }
}

const fn choice(id: u16, key: &'static str, name: &'static str, names: &'static [&'static str], default: i32) -> Info {
    Info { id, key, name, kind: Kind::Choice(names, default) }
}

pub const DELAY_TYPE: u16 = 18;
pub const DELAY_TIME: u16 = 19;
pub const DELAY_RANGE: u16 = 20;
pub const DELAY_TONE: u16 = 21;
pub const DELAY_FEEDBACK: u16 = 22;
pub const DELAY_RETURN: u16 = 23;
pub const RING_WAVE: u16 = 24;
pub const RING_FREQ: u16 = 25;
pub const RING_RETURN: u16 = 26;
pub const VERB_SIZE: u16 = 27;
pub const VERB_DECAY: u16 = 28;
pub const VERB_TONE: u16 = 29;
pub const VERB_VARIATION: u16 = 30;
pub const VERB_RETURN: u16 = 31;
pub const FILTER: u16 = 32;
pub const FILTER_RESO: u16 = 33;
pub const DRIVE_TYPE: u16 = 34;
pub const DRIVE: u16 = 35;
pub const DRIVE_COLOR: u16 = 36;
pub const DRIVE_TONE: u16 = 37;
pub const DRIVE_LEVEL: u16 = 38;
pub const DRIVE_MIX: u16 = 39;
pub const COMP_RATIO: u16 = 40;
pub const COMP_ATTACK: u16 = 41;
pub const COMP_RELEASE: u16 = 42;
pub const COMP_THRESH: u16 = 43;
pub const COMP_GAIN: u16 = 44;
pub const COMP_MIX: u16 = 45;
pub const RING_LFO_WAVE: u16 = 46;
pub const RING_LFO_RATE: u16 = 47;
pub const RING_LFO_DEPTH: u16 = 48;
pub const RING_LFO_SYNC: u16 = 49;
pub const COUNT: usize = 50;

/// Everything but the sends, which are made in `all()`.
const SETTINGS: [Info; COUNT - 18] = [
    choice(DELAY_TYPE, "ent_delay_type", "Delay Type", &DELAY_TYPES, 0),
    amount(DELAY_TIME, "ent_delay_time", "Delay Time", 0.4),
    amount(DELAY_RANGE, "ent_delay_range", "Delay Range", 0.5),
    amount(DELAY_TONE, "ent_delay_tone", "Delay Tone", 0.6),
    amount(DELAY_FEEDBACK, "ent_delay_fbk", "Delay Feedback", 0.4),
    amount(DELAY_RETURN, "ent_delay_return", "Delay Return", 1.0),
    choice(RING_WAVE, "ent_ring_wave", "Ring Wave", &RING_WAVES, 3),
    amount(RING_FREQ, "ent_ring_freq", "Ring Frequency", 0.3),
    amount(RING_RETURN, "ent_ring_return", "Ring Return", 1.0),
    amount(VERB_SIZE, "ent_verb_size", "Reverb Size", 0.5),
    amount(VERB_DECAY, "ent_verb_decay", "Reverb Decay", 0.5),
    amount(VERB_TONE, "ent_verb_tone", "Reverb Tone", 0.5),
    amount(VERB_VARIATION, "ent_verb_var", "Reverb Variation", 0.5),
    amount(VERB_RETURN, "ent_verb_return", "Reverb Return", 1.0),
    amount(FILTER, "ent_filter", "Master Filter", 0.5),
    amount(FILTER_RESO, "ent_filter_reso", "Master Filter Reso", 0.0),
    choice(DRIVE_TYPE, "ent_drive_type", "Drive Type", &DRIVE_TYPES, 0),
    amount(DRIVE, "ent_drive", "Drive", 0.3),
    amount(DRIVE_COLOR, "ent_drive_color", "Drive Color", 1.0),
    amount(DRIVE_TONE, "ent_drive_tone", "Drive Tone", 0.7),
    amount(DRIVE_LEVEL, "ent_drive_level", "Drive Level", 0.5),
    amount(DRIVE_MIX, "ent_drive_mix", "Drive Mix", 0.0),
    choice(COMP_RATIO, "ent_comp_ratio", "Comp Ratio", &COMP_RATIOS, 3),
    amount(COMP_ATTACK, "ent_comp_attack", "Comp Attack", 0.1),
    amount(COMP_RELEASE, "ent_comp_release", "Comp Release", 0.3),
    amount(COMP_THRESH, "ent_comp_thresh", "Comp Threshold", 0.7),
    amount(COMP_GAIN, "ent_comp_gain", "Comp Gain", 0.0),
    amount(COMP_MIX, "ent_comp_mix", "Comp Mix", 0.0),
    choice(RING_LFO_WAVE, "ent_ring_lfo_wave", "Ring LFO Wave", &LFO_WAVES, 0),
    amount(RING_LFO_RATE, "ent_ring_lfo_rate", "Ring LFO Rate", 0.4),
    amount(RING_LFO_DEPTH, "ent_ring_lfo_depth", "Ring LFO Depth", 0.0),
    choice(RING_LFO_SYNC, "ent_ring_lfo_sync", "Ring LFO Sync", &LFO_SYNCS, 0),
];

pub fn send_id(bus: usize, voice: usize) -> u16 {
    SEND + (bus * VOICES.len() + voice) as u16
}

pub enum FxParam {
    Amount(FloatParam),
    Choice(IntParam),
}

impl FxParam {
    pub fn as_ptr(&self) -> ParamPtr {
        match self {
            FxParam::Amount(p) => p.as_ptr(),
            FxParam::Choice(p) => p.as_ptr(),
        }
    }

    /// What goes to the engine.
    pub fn engine_value(&self) -> f32 {
        match self {
            FxParam::Amount(p) => p.value(),
            FxParam::Choice(p) => p.value() as f32,
        }
    }
}

/// The parameters in LXR_FX_* order (index = engine number), with their saved ids.
pub fn params() -> Vec<(String, FxParam)> {
    let mut out = Vec::with_capacity(COUNT);
    for bus in BUSES {
        for voice in VOICES {
            let key = format!("ent_send_{}_{}", bus.to_lowercase(), voice.to_lowercase().replace('/', ""));
            out.push((key, FxParam::Amount(amount_param(format!("{bus} Send {voice}"), 0.0))));
        }
    }
    for info in SETTINGS.iter() {
        debug_assert_eq!(out.len() as u16, info.id, "{} out of order", info.key);
        let p = match info.kind {
            Kind::Amount(default) => {
                let p = amount_param(info.name.to_string(), default);
                FxParam::Amount(match info.id {
                    FILTER => p.with_unit("").with_value_to_string(std::sync::Arc::new(|v| filter_text(v))),
                    RING_LFO_RATE => p.with_unit(" Hz").with_value_to_string(std::sync::Arc::new(|v| format!("{:.2}", lfo_hz(v)))),
                    _ => p,
                })
            }
            Kind::Choice(names, default) => FxParam::Choice(
                IntParam::new(info.name, default, IntRange::Linear { min: 0, max: names.len() as i32 - 1 })
                    .with_value_to_string(std::sync::Arc::new(move |v| names[v.clamp(0, names.len() as i32 - 1) as usize].to_string()))
                    .with_string_to_value(std::sync::Arc::new(move |s| names.iter().position(|n| n.eq_ignore_ascii_case(s.trim())).map(|i| i as i32))),
            ),
        };
        out.push((info.key.to_string(), p));
    }
    debug_assert_eq!(out.len(), COUNT);
    out
}

/// The ring LFO's rate for a RATE setting (lxr_entropia.c).
pub fn lfo_hz(v: f32) -> f32 {
    0.05 * 400f32.powf(v)
}

/// "off", "LP 40" or "HP 40" for a master filter setting.
pub fn filter_text(v: f32) -> String {
    let amount = ((v - 0.5).abs() * 200.0).round();
    match () {
        _ if amount < 1.0 => "off".to_string(),
        _ if v < 0.5 => format!("LP {amount:.0}"),
        _ => format!("HP {amount:.0}"),
    }
}

fn amount_param(name: String, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

/// Host-facing group name per engine number.
pub fn group(id: u16) -> &'static str {
    match id {
        0..=17 => "Entropia Mod Sends",
        DELAY_TYPE..=VERB_RETURN | RING_LFO_WAVE.. => "Entropia Mod FX",
        _ => "Entropia Mod Master",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbering_matches_the_engine() {
        // lxr_bridge.h: LXR_FX_DELAY_TYPE = 18 ... LXR_FX_COUNT
        let header = include_str!("../engine/lxr_bridge.h");
        let start = header.find("LXR_FX_DELAY_TYPE = 18").expect("enum");
        let names: Vec<&str> = header[start..]
            .lines()
            .map(|l| l.trim().split([' ', ',']).next().unwrap_or(""))
            .filter(|n| n.starts_with("LXR_FX_"))
            .take_while(|n| *n != "LXR_FX_COUNT")
            .collect();
        assert_eq!(names.len() + 18, COUNT, "{names:?}");
        for (info, name) in SETTINGS.iter().zip(&names) {
            let expected = 18 + names.iter().position(|n| n == name).unwrap() as u16;
            assert_eq!(info.id, expected, "{name}");
        }
        assert_eq!(params().len(), COUNT);
        let keys: std::collections::HashSet<String> = params().into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys.len(), COUNT, "unique ids");
        assert!(header.contains(&format!("#define LXR_FX_LFO_SYNCS {}", LFO_SYNCS.len())));
    }
}
