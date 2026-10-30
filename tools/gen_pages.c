// Dumps the LXR menu pages (what the LCD shows per voice and sub page) for gen_pages.py.
#include <stdio.h>
#include "menu/menu.h"
#include "menu/MenuText.h"
#include "menu/menuPages.h"
#include "../Src/menu/Cc2Text.c"
#define PROGMEM
#include "value_names.inc"

static void dump_page(const char *name, int page)
{
	for (int sub = 0; sub < NUM_SUB_PAGES; sub++) {
		const Page *p = &menuPages[page][sub];
		const uint8_t *top = &p->top1;
		const uint16_t *bot = &p->bot1;
		for (int slot = 0; slot < 8; slot++) {
			const Name *n = &valueNames[top[slot]];
			printf("%s\t%d\t%d\t%d\t%d\t%s\t%s\t%s\n", name, sub, slot, top[slot], bot[slot],
				shortNames[n->shortName], catNames[n->category], longNames[n->longName]);
		}
	}
}

int main(void)
{
	printf("TEXT_EMPTY\t%d\nTEXT_SKIP\t%d\n", TEXT_EMPTY, TEXT_SKIP);
	const char *voices[] = {"VOICE1", "VOICE2", "VOICE3", "VOICE4", "VOICE5", "VOICE6", "VOICE7"};
	for (int v = 0; v < 7; v++)
		dump_page(voices[v], VOICE1_PAGE + v);
	dump_page("FX", FX_MODE_PAGE);
	for (int i = 0; i < (int)getNumModTargets(); i++) {
		int voice = 0;
		for (int v = 0; v < 6; v++)
			if (i >= modTargetVoiceOffsets[v].start && i <= modTargetVoiceOffsets[v].end)
				voice = v + 1;
		const Name *n = &valueNames[modTargets[i].nameIdx];
		printf("MODTARGET\t%d\t%d\t%d\t%s\t%s\t%s\n", i, modTargets[i].param, voice,
			shortNames[n->shortName], catNames[n->category], longNames[n->longName]);
	}
	return 0;
}
