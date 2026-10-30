#!/bin/bash
# Builds the engine with AddressSanitizer, sweeps the plugin's parameter table and plays every
# embedded kit and every pattern of the embedded projects (param_sweep.c).
set -e
E="$(cd "$(dirname "$0")/.." && pwd)"; R="$(dirname "$E")"; F=$E/firmware/Core; S=$E/shim; O=$(mktemp -d)
grep -oE 'index: [0-9]+, group: "[^"]*", name: "[^"]*", min: -?[0-9]+, max: -?[0-9]+' "$R/src/param_table.rs" \
  | sed -E 's/index: ([0-9]+).*min: (-?[0-9]+), max: (-?[0-9]+)/{\1, \2, \3},/' > "$O/param_ranges.inc"
SRCS=$(find $F/Src/Soundengine -name '*.c' | grep -vE 'cs4344|AudioCodecManager|TriggerOut|/USB/|MIDI/Uart.c|CortexParser|Soundengine/Soundengine.c')
SRCS="$SRCS $F/Src/Preset/initKit.c $F/Src/menu/Cc2Text.c $E/lxr_bridge.c $E/lxr_entropia.c $E/lxr_stubs.c"
n=0; for f in $SRCS; do n=$((n+1)); clang -c -O1 -g -fsanitize=address,undefined -fno-sanitize-recover=address -w -std=gnu11 '-Dsection(x)=used' -include $S/stm32f7xx_hal.h -I$S -I$F/Inc -I$F/Inc/Soundengine -I$F/Inc/Soundengine/DSPAudio "$f" -o "$O/o_${n}.o"; done
clang -O1 -g -fsanitize=address,undefined -I"$O" "$E/test/param_sweep.c" "$O"/o_*.o -o "$O/param_sweep"
"$O/param_sweep"
"$O/param_sweep" "$R/kits/lxr_kits.bin"
"$O/param_sweep" - "$R/kits/lxr_projects.bin"
