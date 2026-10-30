// Dumps the LXR front-panel parameter metadata (datatype + menu value names)
// so gen_params.py can build the plugin's parameter table.
#include <stdio.h>
#include <string.h>
#include "menu/menu.h"
#include "menu/MenuText.h"
#define PROGMEM
uint8_t menu_numSamples = 0;
#include "dtypes_table.inc"

#define DUMP(id, arr) case id: n = arr[0][0]; for (int i = 0; i < n; i++) printf("%s%s", i ? "\t" : "", arr[i + 1]); break;
static void dump_menu(int menuId)
{
	int n = 0;
	switch (menuId) {
	DUMP(MENU_TRANS, transientNames) DUMP(MENU_AUDIO_OUT, outputNames) DUMP(MENU_AUDIO_OUT_FX, outputNamesFx)
	DUMP(MENU_FILTER, filterTypes) DUMP(MENU_SYNC_RATES, syncRateNames) DUMP(MENU_LFO_WAVES, lfoWaveNames)
	DUMP(MENU_RETRIGGER, retriggerNames) DUMP(MENU_FOLLOW, followNames) DUMP(MENU_SEQ_QUANT, quantisationNames)
	DUMP(MENU_MIDI, midiModes) DUMP(MENU_NEXT_PATTERN, nextPatternNames) DUMP(MENU_WAVEFORM, waveformNames)
	DUMP(MENU_ROLL_RATES, rollRateNames) DUMP(MENU_MIDI_ROUTING, midiRoutingNames) DUMP(MENU_MIDI_FILTERING, midiFilterNames)
	DUMP(MENU_PPQ_OUT, ppqOutNames) DUMP(MENU_PPQ_IN, ppqInNames) DUMP(MENU_MIX_FM, mixFmNames)
	DUMP(MENU_FX, fxNames) DUMP(MENU_DIST, distNames) DUMP(MENU_RATIO, ratioNames)
	DUMP(MENU_DELAY, delayNames) DUMP(MENU_REVERB, reverbNames) DUMP(MENU_PAT_MODE, patModeNames)
	DUMP(MENU_STEP_FLAM, stepFlamNames) DUMP(MENU_STEP_SHIFT, stepShiftNames)
	default: break;
	}
}

int main(void)
{
	printf("END_OF_SOUND_PARAMETERS\t%d\n", END_OF_SOUND_PARAMETERS);
	for (int i = 0; i < END_OF_SOUND_PARAMETERS; i++) {
		int dt = parameter_dtypes[i] & 0xff, menuId = parameter_dtypes[i] >> 8;
		printf("%d\t%d\t%d\t", i, dt, menuId);
		if (dt == DTYPE_MENU) dump_menu(menuId);
		printf("\n");
	}
	printf("DTYPES\tB255=%d B127=%d PM100=%d MENU=%d PM63=%d 1B16=%d ONOFF=%d TLFO=%d TVELO=%d VLFO=%d AUTOM=%d 0b1=%d NOTE=%d 0B15=%d 1B64=%d 0B100=%d\n",
		DTYPE_0B255, DTYPE_0B127, DTYPE_PM100, DTYPE_MENU, DTYPE_PM63, DTYPE_1B16, DTYPE_ON_OFF, DTYPE_TARGET_SELECTION_LFO,
		DTYPE_TARGET_SELECTION_VELO, DTYPE_VOICE_LFO, DTYPE_AUTOM_TARGET, DTYPE_0b1, DTYPE_NOTE_NAME, DTYPE_0B15, DTYPE_1B64, DTYPE_0B100);
	return 0;
}
