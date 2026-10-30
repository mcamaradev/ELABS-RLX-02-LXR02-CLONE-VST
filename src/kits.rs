//! LXR kits: the official factory and sound pack kits embedded at build time (kit_table.rs), plus
//! the kits in the user's library folder (library.rs).

use nih_plug::prelude::*;
use crate::kit_table::{KITS, KIT_DATA, KIT_SIZE};
use crate::param_table::{Display, PARAMS};
use crate::ui_pages::MOD_TARGETS;
use crate::LxrParams;

pub struct Kit {
    pub bank: String,
    pub name: String,
    /// front panel parameter values, indexed like the firmware's ParamEnums
    pub data: Vec<u8>,
}

/// All kits: the embedded banks first, then the library folder's (one bank per folder).
pub fn all_kits() -> Vec<Kit> {
    let mut kits: Vec<Kit> = KITS
        .iter()
        .enumerate()
        .map(|(i, (bank, name))| Kit {
            bank: bank.to_string(),
            name: name.to_string(),
            data: KIT_DATA[i * KIT_SIZE..(i + 1) * KIT_SIZE].to_vec(),
        })
        .collect();
    // a library kit with a built-in one's sound (saved from it, or the user's copy of it) is shown once
    let built_in: std::collections::HashSet<Vec<u8>> = kits.iter().map(|k| k.data.clone()).collect();
    kits.extend(
        crate::library::kits().into_iter().filter(|(_, _, data)| !built_in.contains(data)).map(|(bank, name, data)| Kit { bank, name, data }),
    );
    kits
}

/// The value a kit sets for a parameter, normalised the way the firmware does when loading.
pub fn kit_value(data: &[u8], index: u16, min: i32, max: i32, display: &Display) -> i32 {
    let v = data.get(index as usize).copied().unwrap_or(0) as i32;
    // kits saved before the mod target list changed may point past it (preset_readDrumsetData)
    if matches!(display, Display::ModTarget) && v as usize >= MOD_TARGETS.len() {
        return 0;
    }
    v.clamp(min, max)
}

/// The lock bit of a parameter group: the six voices, then the FX (the master isn't locked).
pub fn lock_bit(group: &str) -> Option<u8> {
    ["Drum 1", "Drum 2", "Drum 3", "Snare", "Cymbal", "Hihat", "FX"].iter().position(|g| *g == group).map(|i| 1 << i)
}

/// Sets every exposed sound parameter to the kit's value, as host-visible parameter changes;
/// locked voices keep their sound.
pub fn load(setter: &ParamSetter, params: &LxrParams, kit: &Kit) {
    let locks = params.voice_locks.load(std::sync::atomic::Ordering::Acquire);
    for (info, param) in PARAMS.iter().zip(&params.sound) {
        if lock_bit(info.group).is_some_and(|bit| locks & bit != 0) {
            continue;
        }
        let v = kit_value(&kit.data, info.index, info.min, info.max, &info.display);
        if param.value() != v {
            setter.begin_set_parameter(param);
            setter.set_parameter(param, v);
            setter.end_set_parameter(param);
        }
    }
    if let Ok(mut name) = params.kit_name.lock() {
        *name = kit.name.clone();
    }
}
