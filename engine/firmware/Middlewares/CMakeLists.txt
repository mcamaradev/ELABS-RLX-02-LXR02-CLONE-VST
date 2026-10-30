# Copyright (C) 2026 Erica Synths
# SPDX-License-Identifier: GPL-3.0-only

# Middleware: FatFs (SD card) and the USB device stack (customised to USB-MIDI).
#
# Both are built as OBJECT libraries, like the rest of the firmware, so every
# object is linked directly exactly as in the original STM32CubeIDE build
# (a static archive could silently prefer a __weak default over an override).

# --- FatFs + CubeMX SD glue (FATFS/App, FATFS/Target) -------------------------
add_library(fatfs OBJECT
    Third_Party/FatFs/src/diskio.c
    Third_Party/FatFs/src/ff.c
    Third_Party/FatFs/src/ff_gen_drv.c
    Third_Party/FatFs/src/option/syscall.c
    ${PROJECT_SOURCE_DIR}/FATFS/App/fatfs.c
    ${PROJECT_SOURCE_DIR}/FATFS/Target/bsp_driver_sd.c
    ${PROJECT_SOURCE_DIR}/FATFS/Target/fatfs_platform.c
    ${PROJECT_SOURCE_DIR}/FATFS/Target/sd_diskio.c
)

target_include_directories(fatfs PUBLIC
    Third_Party/FatFs/src
    ${PROJECT_SOURCE_DIR}/FATFS/App
    ${PROJECT_SOURCE_DIR}/FATFS/Target
)

target_link_libraries(fatfs PUBLIC stm32f7_hal)
target_compile_options(fatfs PRIVATE -w)

# --- USB device library + USB_DEVICE_MIDI application glue --------------------
# Note: the "CDC" class in Middlewares/JS was rewritten into a USB-MIDI class and
# includes application headers (Soundengine/MIDI/MidiMessages.h from Core/Inc).
add_library(usb_midi OBJECT
    JS/STM32_USB_Device_Library/Core/Src/usbd_core.c
    JS/STM32_USB_Device_Library/Core/Src/usbd_ctlreq.c
    JS/STM32_USB_Device_Library/Core/Src/usbd_ioreq.c
    JS/STM32_USB_Device_Library/Class/CDC/Src/usbd_cdc.c
    ${PROJECT_SOURCE_DIR}/USB_DEVICE_MIDI/App/usb_device.c
    ${PROJECT_SOURCE_DIR}/USB_DEVICE_MIDI/App/usbd_cdc_if.c
    ${PROJECT_SOURCE_DIR}/USB_DEVICE_MIDI/App/usbd_desc.c
    ${PROJECT_SOURCE_DIR}/USB_DEVICE_MIDI/Target/usbd_conf.c
)

target_include_directories(usb_midi PUBLIC
    JS/STM32_USB_Device_Library/Core/Inc
    JS/STM32_USB_Device_Library/Class/CDC/Inc
    ${PROJECT_SOURCE_DIR}/USB_DEVICE_MIDI/App
    ${PROJECT_SOURCE_DIR}/USB_DEVICE_MIDI/Target
)

target_link_libraries(usb_midi PUBLIC stm32f7_hal)
target_compile_options(usb_midi PRIVATE -w)
