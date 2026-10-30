#!/bin/bash
# Builds the engine natively with clang and links the render test.
set -e
E="$(cd "$(dirname "$0")/.." && pwd)"; F=$E/firmware/Core; S=$E/shim; O=${1:-/tmp/lxr_test_build}
rm -rf "$O"; mkdir -p "$O"
SRCS=$(find $F/Src/Soundengine -name '*.c' | grep -vE 'cs4344|AudioCodecManager|TriggerOut|/USB/|MIDI/Uart.c|CortexParser|Soundengine/Soundengine.c')
SRCS="$SRCS $F/Src/Preset/initKit.c $F/Src/menu/Cc2Text.c $E/lxr_bridge.c $E/lxr_entropia.c $E/lxr_stubs.c $E/test/render_test.c"
n=0; for f in $SRCS; do n=$((n+1)); clang -c -O2 -w -std=gnu11 -ffp-contract=off '-Dsection(x)=used' -include $S/stm32f7xx_hal.h -I$S -I$F/Inc -I$F/Inc/Soundengine -I$F/Inc/Soundengine/DSPAudio "$f" -o "$O/$n_$(basename $f .c)_$n.o"; done
clang "$O"/*.o -o "$O/render_test"
echo "$O/render_test"
