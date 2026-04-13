# Makefile for ATSAM3X8E firmware (Arduino Due)
# Builds firmware.bin compatible with BOSSA flashing.
# Recipe extracted from ATSAM3X8E_FW.cproj (Atmel/Microchip Studio project).
#
# Usage:
#   make             -> build/firmware.bin
#   make clean
#   make flash       -> bossac via /dev/arduino_due_prog (host must press ERASE first)

PROJECT  := phycmd_fw
BUILD    := build
ELF      := $(BUILD)/$(PROJECT).elf
BIN      := $(BUILD)/$(PROJECT).bin
HEX      := $(BUILD)/$(PROJECT).hex
MAP      := $(BUILD)/$(PROJECT).map

CROSS    := arm-none-eabi-
CC       := $(CROSS)gcc
OBJCOPY  := $(CROSS)objcopy
SIZE     := $(CROSS)size

# ---- Architecture ----
ARCH_FLAGS := -mcpu=cortex-m3 -mthumb

# ---- Defines (from .cproj DefSymbols) ----
DEFINES := \
  -DARM_MATH_CM3=true \
  -DBOARD=ARDUINO_DUE_X \
  -D__SAM3X8E__ \
  -Dprintf=iprintf \
  -DUDD_ENABLE

# ---- Include paths (from .cproj IncludePaths, minus ToolchainDir entries which are bundled in src/ASF) ----
INCLUDES := \
  -Isrc \
  -Isrc/config \
  -Isrc/ASF/common/boards \
  -Isrc/ASF/common/services/clock \
  -Isrc/ASF/common/services/delay \
  -Isrc/ASF/common/services/fifo \
  -Isrc/ASF/common/services/gpio \
  -Isrc/ASF/common/services/ioport \
  -Isrc/ASF/common/services/serial \
  -Isrc/ASF/common/services/serial/sam_uart \
  -Isrc/ASF/common/services/sleepmgr \
  -Isrc/ASF/common/services/usb \
  -Isrc/ASF/common/services/usb/udc \
  -Isrc/ASF/common/utils \
  -Isrc/ASF/common/utils/stdio/stdio_serial \
  -Isrc/ASF/sam/boards \
  -Isrc/ASF/sam/boards/arduino_due_x \
  -Isrc/ASF/sam/drivers/adc \
  -Isrc/ASF/sam/drivers/can \
  -Isrc/ASF/sam/drivers/dacc \
  -Isrc/ASF/sam/drivers/dmac \
  -Isrc/ASF/sam/drivers/pdc \
  -Isrc/ASF/sam/drivers/pio \
  -Isrc/ASF/sam/drivers/pmc \
  -Isrc/ASF/sam/drivers/pwm \
  -Isrc/ASF/sam/drivers/rtc \
  -Isrc/ASF/sam/drivers/rtt \
  -Isrc/ASF/sam/drivers/tc \
  -Isrc/ASF/sam/drivers/trng \
  -Isrc/ASF/sam/drivers/uart \
  -Isrc/ASF/sam/drivers/uotghs \
  -Isrc/ASF/sam/drivers/usart \
  -Isrc/ASF/sam/drivers/wdt \
  -Isrc/ASF/sam/utils \
  -Isrc/ASF/sam/utils/cmsis/sam3x/include \
  -Isrc/ASF/sam/utils/cmsis/sam3x/source/templates \
  -Isrc/ASF/sam/utils/header_files \
  -Isrc/ASF/sam/utils/preprocessor \
  -Isrc/ASF/thirdparty/CMSIS/Include

# ---- Sources (from .cproj <Compile Include="..."/>) ----
SRCS := \
  src/main.c \
  src/udi_vendor.c \
  src/waveform.c \
  src/ASF/common/services/clock/sam3x/sysclk.c \
  src/ASF/common/services/delay/sam/cycle_counter.c \
  src/ASF/common/services/fifo/fifo.c \
  src/ASF/common/services/serial/usart_serial.c \
  src/ASF/common/services/sleepmgr/sam/sleepmgr.c \
  src/ASF/common/services/usb/udc/udc.c \
  src/ASF/common/utils/interrupt/interrupt_sam_nvic.c \
  src/ASF/common/utils/stdio/read.c \
  src/ASF/common/utils/stdio/write.c \
  src/ASF/sam/boards/arduino_due_x/init.c \
  src/ASF/sam/drivers/adc/adc.c \
  src/ASF/sam/drivers/adc/adc_sam3u.c \
  src/ASF/sam/drivers/can/can.c \
  src/ASF/sam/drivers/dacc/dacc.c \
  src/ASF/sam/drivers/dmac/dmac.c \
  src/ASF/sam/drivers/pdc/pdc.c \
  src/ASF/sam/drivers/pio/pio.c \
  src/ASF/sam/drivers/pio/pio_handler.c \
  src/ASF/sam/drivers/pmc/pmc.c \
  src/ASF/sam/drivers/pmc/sleep.c \
  src/ASF/sam/drivers/pwm/pwm.c \
  src/ASF/sam/drivers/rtc/rtc.c \
  src/ASF/sam/drivers/rtt/rtt.c \
  src/ASF/sam/drivers/tc/tc.c \
  src/ASF/sam/drivers/trng/trng.c \
  src/ASF/sam/drivers/uart/uart.c \
  src/ASF/sam/drivers/uotghs/uotghs_device.c \
  src/ASF/sam/drivers/usart/usart.c \
  src/ASF/sam/drivers/wdt/wdt.c \
  src/ASF/sam/utils/cmsis/sam3x/source/templates/exceptions.c \
  src/ASF/sam/utils/cmsis/sam3x/source/templates/gcc/startup_sam3x.c \
  src/ASF/sam/utils/cmsis/sam3x/source/templates/system_sam3x.c \
  src/ASF/sam/utils/syscalls/gcc/syscalls.c

OBJS := $(SRCS:%.c=$(BUILD)/%.o)
DEPS := $(OBJS:.o=.d)

# ---- Flags ----
OPT      := -O2
WARN     := -Wall -Wstrict-prototypes -Wmissing-prototypes \
            -Werror-implicit-function-declaration -Wpointer-arith \
            -Wchar-subscripts -Wcomment -Wformat=2 -Wmain -Wparentheses \
            -Wsequence-point -Wreturn-type -Wswitch -Wtrigraphs \
            -Wno-unused -Wuninitialized -Wunknown-pragmas \
            -Wno-float-equal -Wundef -Wno-shadow \
            -Wbad-function-cast -Wwrite-strings -Wsign-compare \
            -Wno-aggregate-return -Wmissing-declarations -Wformat \
            -Wmissing-format-attribute -Wno-deprecated-declarations \
            -Wno-packed -Wredundant-decls -Wnested-externs \
            -Wno-long-long -Wunreachable-code -Wcast-align \
            -Wno-error -Wno-implicit-fallthrough -Wno-redundant-decls \
            -Wno-array-bounds -Wno-stringop-overflow -Wno-stringop-overread

CFLAGS := $(ARCH_FLAGS) $(OPT) -std=gnu99 \
          -ffunction-sections -fdata-sections \
          -fno-strict-aliasing -fdata-sections \
          -pipe \
          --param max-inline-insns-single=500 \
          $(WARN) $(DEFINES) $(INCLUDES) \
          -MMD -MP

LD_SCRIPT := src/ASF/sam/utils/linker_scripts/sam3x/sam3x8/gcc/flash.ld
LIB_DIRS  := -Lsrc/ASF/thirdparty/CMSIS/Lib/GCC
LIBS      := -larm_cortexM3l_math -lm

LDFLAGS := $(ARCH_FLAGS) \
           -Wl,--entry=Reset_Handler \
           -Wl,--gc-sections \
           -Wl,--cref \
           -Wl,-Map=$(MAP) \
           -T$(LD_SCRIPT) \
           --specs=nosys.specs \
           $(LIB_DIRS)

# ---- Rules ----
.PHONY: all clean flash size

all: $(BIN) $(HEX) size

$(BUILD)/%.o: %.c
	@mkdir -p $(dir $@)
	@echo "  CC    $<"
	@$(CC) $(CFLAGS) -c $< -o $@

$(ELF): $(OBJS)
	@mkdir -p $(dir $@)
	@echo "  LD    $@"
	@$(CC) $(LDFLAGS) -o $@ $(OBJS) $(LIBS)

$(BIN): $(ELF)
	@echo "  BIN   $@"
	@$(OBJCOPY) -O binary $< $@

$(HEX): $(ELF)
	@echo "  HEX   $@"
	@$(OBJCOPY) -O ihex $< $@

size: $(ELF)
	@$(SIZE) $<

flash: $(BIN)
	@echo "  FLASH $(BIN) via /dev/arduino_due_prog"
	@echo "        (press the Arduino Due ERASE button BEFORE running this if the board hangs)"
	bossac --port=arduino_due_prog -e -w -v -b -R $(BIN)

clean:
	rm -rf $(BUILD)

-include $(DEPS)
