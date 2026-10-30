#!/bin/bash
# Regenerates src/param_table.rs and src/ui_pages.rs from the firmware sources in engine/firmware.
set -e
R="$(cd "$(dirname "$0")/.." && pwd)"; F=$R/engine/firmware/Core; S=$R/engine/shim
T=$(mktemp -d)
CFLAGS="-w -std=gnu11 -Dsection(x)=used -include $S/stm32f7xx_hal.h -I$S -I$F/Inc"
sed -n '/^const enum Datatypes PROGMEM parameter_dtypes/,/^};/p' "$F/Src/menu/menu.c" > "$T/dtypes_table.inc"
clang $CFLAGS -I"$T" "$R/tools/gen_params.c" -o "$T/gen_params" && "$T/gen_params" > "$T/params_dump.txt"
python3 -I "$R/tools/enum_names_template.py" "$F/Inc/Parameters.h" "$T/enum_names.c"
clang $CFLAGS "$T/enum_names.c" -o "$T/enum_names" && "$T/enum_names" > "$T/enum_names.txt"
sed -n '/^const Name valueNames/,/^};/p' "$F/Src/menu/menu.c" > "$T/value_names.inc"
clang $CFLAGS -I"$T" "$R/tools/gen_pages.c" -o "$T/gen_pages" && "$T/gen_pages" > "$T/pages.txt"
python3 -I "$R/tools/gen_params.py" "$T/enum_names.txt" "$T/params_dump.txt" "$T/pages.txt" "$F/Src/Preset/initKit.c" "$R/src/param_table.rs"
python3 -I "$R/tools/gen_pages.py" "$T/pages.txt" "$R/src/ui_pages.rs"
rm -rf "$T"
