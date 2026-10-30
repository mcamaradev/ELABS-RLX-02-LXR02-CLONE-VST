// Stand-ins for the firmware parts that are not compiled into the plugin
// (front panel, LCD menu, UART/USB MIDI, SD card and the step sequencer).
// The sound engine calls into these; in the plugin they do nothing or return neutral values.
#include "stm32f7xx_hal.h"
#include "Parameters.h"
#include "Soundengine/MIDI/MidiMessages.h"
#include "Soundengine/Sequencer/sequencer.h"

// Front panel edit buffer. The engine's modulation code reads the unmodulated values from here.
uint8_t activeParameters[NUM_PARAMS];

// Per-voice volume faders (0..1), set by the plugin.
float lxr_faderVolume[6] = {1.f, 1.f, 1.f, 1.f, 1.f, 1.f};
float adc_getFaderVolume(uint8_t idx) { return idx < 6 ? lxr_faderVolume[idx] : 1.f; }

volatile uint32_t systick_ticks;

// menu / front panel
uint8_t frontParser_activeTrack;
uint8_t menu_activePage;
uint8_t menu_getActiveVoice() { return 0; }
uint8_t menu_getSubPage() { return 0; }
uint8_t menu_getViewedPattern() { return 0; }
void menu_repaint() {}
void menu_switchSubPage(uint8_t subPageNr, uint8_t toggle) { (void)subPageNr; (void)toggle; }

// MIDI out. voiceControl_noteOn() tells the front panel which voice played (NOTE_ON, voice, 0);
// that is kept for the plugin's pad LEDs.
volatile uint8_t lxr_triggered;
void uart_sendFrontpanelByte(uint8_t data)
{
	static uint8_t last;
	if (last == NOTE_ON && data < 7)
		lxr_triggered |= 1 << data;
	last = data;
}
void uart_sendMidi(MidiMsg msg) { (void)msg; }
void usb_sendMidi(MidiMsg msg) { (void)msg; }


// Front panel parts the sequencer reports to (LEDs, menu, presets). The plugin's panel reads the
// sequencer state itself, and loads the kit that goes with a pattern on its own.
#include "menu/menu.h"
#include "Preset/PresetManager.h"
uint8_t frontParser_shownPattern;
uint8_t menu_activeBar;
uint8_t menu_loadedPresetNr[NUM_CURRENT];
uint8_t menu_playedPattern;
uint8_t menu_selectedPresetNr[NUM_PRESET_LOCATIONS];
uint8_t menu_shownPattern;
uint8_t menu_songMenuPosition;
uint8_t midi_process_block;
uint8_t trigger_clockReceived;
uint8_t buttonHandler_getMode() { return 0; }
void led_blinkHandler() {}
void led_resetBlinkTimer() {}
void led_clearActive_step() {}
void led_clearAllBlinkLeds() {}
void led_clearSequencerLeds() {}
uint8_t led_idx2StepLed(uint8_t idx) { return idx; }
void led_initPatternSelectLeds() {}
void led_setActive_step(uint8_t stepNr) { (void)stepNr; }
void led_setBlinkLed(const uint8_t ledNr, const uint8_t onOff) { (void)ledNr; (void)onOff; }
void led_setValue(uint8_t val, uint8_t ledNr) { (void)val; (void)ledNr; }
void led_updateSeqLeds() {}
void menu_repaintAll() {}
void menu_setShownPattern(uint8_t patternNr) { (void)patternNr; }
void preset_loadDrumsetRam(uint8_t presetNr, uint8_t isMorph, uint8_t sendImmediate) { (void)presetNr; (void)isMorph; (void)sendImmediate; }
void preset_touchTimer(PresetTimer type) { (void)type; }
void trigger_reset(uint8_t value) { (void)value; }
void trigger_clockTick(uint8_t pinStatus, uint8_t ppqCnt) { (void)pinStatus; (void)ppqCnt; }
void uart_sendFrontpanelSysExByte(uint8_t data) { (void)data; }
