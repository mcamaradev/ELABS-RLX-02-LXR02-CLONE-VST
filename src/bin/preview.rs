//! Standalone window for checking the panel without a DAW.
use elabs_lxr::Lxr;
use nih_plug::prelude::*;

fn main() {
    nih_export_standalone::<Lxr>();
}
