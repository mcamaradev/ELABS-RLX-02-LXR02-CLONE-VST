/* FatFs types the firmware headers refer to. There is no SD card in the plugin: kits and patterns
   come from the plugin, so nothing here is ever called. */
#pragma once
#include <stdint.h>
typedef unsigned int UINT;
typedef unsigned char BYTE;
typedef uint32_t DWORD;
typedef uint32_t FSIZE_t;
typedef enum {
	FR_OK = 0, FR_DISK_ERR, FR_INT_ERR, FR_NOT_READY, FR_NO_FILE, FR_NO_PATH, FR_INVALID_NAME,
	FR_DENIED, FR_EXIST, FR_INVALID_OBJECT, FR_WRITE_PROTECTED, FR_INVALID_DRIVE, FR_NOT_ENABLED,
	FR_NO_FILESYSTEM, FR_MKFS_ABORTED, FR_TIMEOUT, FR_LOCKED, FR_NOT_ENOUGH_CORE,
	FR_TOO_MANY_OPEN_FILES, FR_INVALID_PARAMETER
} FRESULT;
typedef struct { int dummy; } FIL, DIR, FATFS, FILINFO;
