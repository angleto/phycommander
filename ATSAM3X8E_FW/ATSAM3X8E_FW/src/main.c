/*
 * SPDX-License-Identifier: GPL-3.0-or-later
 * SPDX-FileCopyrightText: 2014-2026 Angelo Leto <angelo@leto.blue>
 */
/**
 * \file
 *
 * \brief PhyCommander firmware — Step 3 (PhyCMD-64 protocol on vendor bulk).
 *
 * Hardware initialisation (GPIO, ADC, DAC, SysTick) runs once in
 * main(). The USB transport (vendor class, two bulk endpoints) is
 * managed by udi_vendor.c. When a 64-byte frame arrives on the OUT
 * endpoint, the USB ISR callback in udi_vendor.c calls
 * process_command_frame() here, which validates the PhyCMD-64 CRC,
 * applies digital/DAC outputs, reads digital/ADC inputs, builds the
 * 64-byte status response in the caller-supplied TX buffer, and
 * returns. The ISR then queues that buffer for BULK IN transmission.
 *
 * The entire protocol round-trip is IRQ-driven — main()'s infinite
 * loop is an idle spin (no WFI, per Bug 3 from the investigation).
 */

#include <asf.h>
#include <string.h>
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

#include "waveform.h"

/* ============================================================
 *  Protocol constants (must match physerver/src/protocol/)
 * ============================================================ */

#define MSG_SIZE             64
#define COMMAND_HEADER       0xAA55u
#define STATUS_HEADER        0x55AAu
#define CRC_OVER_CMD_BYTES   14u
#define CRC_OVER_STAT_BYTES  24u
#define DAC_MAX              4095u

/* Command flags */
#define FLAG_ADC_ENABLE        (1u << 0)
#define FLAG_DAC_ENABLE        (1u << 1)
#define FLAG_PWM_ENABLE        (1u << 2)
#define FLAG_RESET_SEQ         (1u << 3)
#define FLAG_WATCHDOG_DISABLE  (1u << 4)

/* Status flags */
#define STATUS_ADC_ACTIVE         (1u << 0)
#define STATUS_DAC_ACTIVE         (1u << 1)
#define STATUS_PWM_ACTIVE         (1u << 2)
#define STATUS_ERROR_FLAG         (1u << 3)
#define STATUS_WATCHDOG_TRIGGERED (1u << 4)
#define STATUS_USB_CONFIGURED     (1u << 5)
#define STATUS_OVERRUN            (1u << 6)

/* ============================================================
 *  Protocol wire types (packed, 64 B each)
 * ============================================================ */

typedef struct __attribute__((packed)) {
	uint16_t header;        /* 0xAA55 */
	uint16_t digital_out;
	uint16_t dac0;
	uint16_t dac1;
	uint16_t pwm0;
	uint16_t pwm1;
	uint8_t  flags;
	uint8_t  seq_num;
	uint16_t crc;
	uint8_t  reserved[48];
} command_msg_t;

typedef struct __attribute__((packed)) {
	uint16_t header;        /* 0x55AA */
	uint16_t digital_in;
	uint16_t digital_out;
	uint16_t adc[12];       /* Due A0..A11 (bumped from 8 on 2026-04-25) */
	uint8_t  status_flags;
	uint8_t  seq_num;
	uint16_t crc;
	uint16_t loop_time_us;
	uint32_t uptime_ms;
	uint16_t error_count;
	uint8_t  reserved[22];
} status_msg_t;

_Static_assert(sizeof(command_msg_t) == MSG_SIZE, "command_msg_t != 64");
_Static_assert(sizeof(status_msg_t)  == MSG_SIZE, "status_msg_t != 64");

/* ============================================================
 *  CRC-16-CCITT (poly=0x1021, init=0xFFFF, MSB-first)
 * ============================================================ */

static uint16_t crc16_ccitt(const uint8_t *data, size_t len)
{
	uint16_t crc = 0xFFFFu;
	for (size_t i = 0; i < len; i++) {
		crc ^= ((uint16_t)data[i]) << 8;
		for (int b = 0; b < 8; b++) {
			if (crc & 0x8000u)
				crc = (uint16_t)((crc << 1) ^ 0x1021u);
			else
				crc = (uint16_t)(crc << 1);
		}
	}
	return crc;
}

/* ============================================================
 *  Protocol state (persistent across frames)
 * ============================================================ */

static uint8_t  s_last_seq    = 0xFFu;
static uint16_t s_error_count = 0;
static volatile uint32_t s_uptime_ms = 0;

/* Cached status frame (lazy rebuild).
 *
 * build_status_frame() is called once per iso IN microframe
 * completion (~8 kHz) but the underlying state only materially
 * changes when SysTick bumps s_uptime_ms (1 kHz) or when a command
 * is processed (<=1 kHz). The other ~7/8 microframes build a frame
 * identical to the previous one, including the CRC. The cache here
 * lets build_status_frame() skip the ~150 cycles of CRC work on
 * those hits: dirty==false means the cached 64 bytes are authoritative.
 *
 * Dirty is set from: SysTick (1 kHz) to pick up uptime_ms, DIN
 * polling, and any state that changes at the 1-kHz-main-loop rate;
 * and from apply_command_frame() to pick up the seq echo and
 * loop_time_us immediately.
 *
 * Concurrency: all writers are in ISR context. The rebuild path
 * (inside build_status_frame) clears dirty FIRST, then rebuilds —
 * if a higher-priority ISR sets dirty during the rebuild, the
 * next call simply rebuilds again. No lost updates.
 */
static COMPILER_WORD_ALIGNED uint8_t s_status_cached[MSG_SIZE];
static volatile bool s_status_dirty = true;

/* Heartbeat LED on D13 (PB27, the on-board "L" LED).
 *
 * Visual semantics (read this from across the lab bench):
 *   - slow 1 Hz blink (500 ms ON / 500 ms OFF): firmware running,
 *     iso link up, USB configured.
 *   - fast ~4 Hz blink (~125 ms toggle): USB not yet configured
 *     (device enumerating or host-side physerver not running yet).
 *   - solid OFF: firmware hung — SysTick isn't ticking.
 *   - solid ON: firmware has detected an unrecoverable error.
 *
 * Controlled entirely from SysTick_Handler below; the pin is
 * configured as PIO output at the end of main() via heartbeat_init().
 */
#define HEARTBEAT_PIN_MASK    (1u << 27)
volatile bool s_heartbeat_enumerated = false;        /* set by udi_vendor on SET_CONFIGURATION (non-static: also read in udi_vendor.c) */
static volatile bool s_heartbeat_error      = false; /* set on unrecoverable firmware error */

/* Watchdog kick cadence, in SysTick ticks (= milliseconds). Picked
 * well below the WDT_MR.WDV timeout (~2 s) so any drop in tick rate
 * of up to 4× still pets the dog before it bites. */
#define WDT_KICK_EVERY_MS  500u

void SysTick_Handler(void);
void SysTick_Handler(void)
{
	s_uptime_ms++;
	waveform_systick_1ms();    /* drives PULSE_TRIG cooldown / pulse end */

	/* Invalidate the cached status frame every 1 ms: at a minimum
	 * s_uptime_ms just changed, and this rate is the natural bound
	 * for the DIN / ADC sampling path too. Iso IN builds that land
	 * within this millisecond use the cached 64 bytes (~0.2 µs
	 * memcpy); the one that lands right after SysTick pays the
	 * full rebuild cost (~2.2 µs). See s_status_cached. */
	s_status_dirty = true;

	/* Heartbeat LED state machine.
	 *  error  → solid ON
	 *  enumerated → 1 Hz square (1000 ms period, 50 % duty)
	 *  otherwise  → 4 Hz square (250 ms period, 50 % duty)
	 * Solid OFF is reserved for "SysTick stopped entirely". */
	if (s_heartbeat_error) {
		PIOB->PIO_SODR = HEARTBEAT_PIN_MASK;
	} else {
		uint32_t period_ms = s_heartbeat_enumerated ? 1000u : 250u;
		uint32_t phase = s_uptime_ms % period_ms;
		if (phase < (period_ms / 2u)) PIOB->PIO_SODR = HEARTBEAT_PIN_MASK;
		else                          PIOB->PIO_CODR = HEARTBEAT_PIN_MASK;
	}

	/* Front-panel reset button on D3 (PC28). Active-low (button
	 * shorts the pin to GND), internal pull-up provides the idle
	 * HIGH. We require ~50 consecutive LOW samples (= 50 ms with
	 * the SysTick at 1 kHz) before triggering reset to debounce
	 * mechanical contact bounce and reject EMI spikes shorter than
	 * a tens-of-milliseconds press. The reset itself writes
	 * RSTC_CR = key(0xA5) | PROCRST | PERRST | EXTRST, the same
	 * full-system reset the flash script issues over SAM-BA. */
	#define RESET_BTN_PIO         PIOC
	#define RESET_BTN_MASK        (1u << 28)
	#define RESET_BTN_DEBOUNCE_MS 50u
	static uint32_t s_reset_btn_low_ms = 0;
	if ((RESET_BTN_PIO->PIO_PDSR & RESET_BTN_MASK) == 0) {
		s_reset_btn_low_ms++;
		if (s_reset_btn_low_ms >= RESET_BTN_DEBOUNCE_MS) {
			RSTC->RSTC_CR = RSTC_CR_KEY(0xA5u)
			              | RSTC_CR_PROCRST
			              | RSTC_CR_PERRST
			              | RSTC_CR_EXTRST;
			while (1) { /* CPU resets here */ }
		}
	} else {
		s_reset_btn_low_ms = 0;
	}

	/* Kick the watchdog periodically. If the main loop, USB ISR,
	 * DACC ISR, or SysTick itself wedges for more than ~2 s the
	 * WDT triggers a hard reset — the host side detects the USB
	 * disconnect and re-opens the device automatically. */
	if ((s_uptime_ms % WDT_KICK_EVERY_MS) == 0) {
		/* WDT_CR_KEY(0xA5) is the WDT password required on every
		 * write; any other value leaves WDT_CR unchanged. */
		WDT->WDT_CR = WDT_CR_KEY(0xA5u) | WDT_CR_WDRSTT;
	}
}

/* ============================================================
 *  Digital I/O (16 in + 16 out, PIO direct register access)
 * ============================================================ */

static Pio *s_dig_in_ports[PHYCMD_DIGITAL_INPUT_NUM];
static Pio *s_dig_out_ports[PHYCMD_DIGITAL_OUTPUT_NUM];

static void init_dig_in_ports(void)
{
	Pio **a = s_dig_in_ports;
	a[0]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_0  >> 5)));
	a[1]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_1  >> 5)));
	a[2]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_2  >> 5)));
	a[3]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_3  >> 5)));
	a[4]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_4  >> 5)));
	a[5]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_5  >> 5)));
	a[6]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_6  >> 5)));
	a[7]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_7  >> 5)));
	a[8]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_8  >> 5)));
	a[9]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_9  >> 5)));
	a[10] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_10 >> 5)));
	a[11] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_11 >> 5)));
	a[12] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_12 >> 5)));
	a[13] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_13 >> 5)));
	a[14] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_14 >> 5)));
	a[15] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_15 >> 5)));
}

static void init_dig_out_ports(void)
{
	Pio **a = s_dig_out_ports;
	a[0]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_0  >> 5)));
	a[1]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_1  >> 5)));
	a[2]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_2  >> 5)));
	a[3]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_3  >> 5)));
	a[4]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_4  >> 5)));
	a[5]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_5  >> 5)));
	a[6]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_6  >> 5)));
	a[7]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_7  >> 5)));
	a[8]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_8  >> 5)));
	a[9]  = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_9  >> 5)));
	a[10] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_10 >> 5)));
	a[11] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_11 >> 5)));
	a[12] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_12 >> 5)));
	a[13] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_13 >> 5)));
	a[14] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_14 >> 5)));
	a[15] = (Pio*)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_15 >> 5)));
}

uint16_t get_dig_in_value(void);   /* externally linkable: also called from waveform.c */
uint16_t get_dig_in_value(void)
{
	uint16_t v = 0;
	for (int i = 0; i < 16; i++) {
		static const uint32_t pins[] = {
			PHYCMD_DIGITAL_INPUT_0,  PHYCMD_DIGITAL_INPUT_1,
			PHYCMD_DIGITAL_INPUT_2,  PHYCMD_DIGITAL_INPUT_3,
			PHYCMD_DIGITAL_INPUT_4,  PHYCMD_DIGITAL_INPUT_5,
			PHYCMD_DIGITAL_INPUT_6,  PHYCMD_DIGITAL_INPUT_7,
			PHYCMD_DIGITAL_INPUT_8,  PHYCMD_DIGITAL_INPUT_9,
			PHYCMD_DIGITAL_INPUT_10, PHYCMD_DIGITAL_INPUT_11,
			PHYCMD_DIGITAL_INPUT_12, PHYCMD_DIGITAL_INPUT_13,
			PHYCMD_DIGITAL_INPUT_14, PHYCMD_DIGITAL_INPUT_15,
		};
		v |= ((s_dig_in_ports[i]->PIO_PDSR >> (pins[i] & 0x1F)) & 1u) << i;
	}
	return v;
}

/* Apply a partial DOUT update from waveform.c reactive modes. Only
 * the bits in `mask` are touched; the rest keep whatever the
 * streaming Command frame last wrote them. The pin index table is
 * defined as a function-local in set_dig_out_value() below — we
 * duplicate it here rather than promoting it to a file-static so
 * that any future renumbering stays mechanically obvious by
 * sitting next to its consumer. */
void reactive_dout_write(uint16_t mask, uint16_t bits);
void reactive_dout_write(uint16_t mask, uint16_t bits)
{
	static const uint32_t pins[] = {
		PHYCMD_DIGITAL_OUTPUT_0,  PHYCMD_DIGITAL_OUTPUT_1,
		PHYCMD_DIGITAL_OUTPUT_2,  PHYCMD_DIGITAL_OUTPUT_3,
		PHYCMD_DIGITAL_OUTPUT_4,  PHYCMD_DIGITAL_OUTPUT_5,
		PHYCMD_DIGITAL_OUTPUT_6,  PHYCMD_DIGITAL_OUTPUT_7,
		PHYCMD_DIGITAL_OUTPUT_8,  PHYCMD_DIGITAL_OUTPUT_9,
		PHYCMD_DIGITAL_OUTPUT_10, PHYCMD_DIGITAL_OUTPUT_11,
		PHYCMD_DIGITAL_OUTPUT_12, PHYCMD_DIGITAL_OUTPUT_13,
		PHYCMD_DIGITAL_OUTPUT_14, PHYCMD_DIGITAL_OUTPUT_15,
	};
	for (uint8_t i = 0; i < 16; i++) {
		if (!(mask & (1u << i))) continue;
		Pio *p = s_dig_out_ports[i];
		uint32_t pin_mask = 1u << (pins[i] & 0x1F);
		if (bits & (1u << i)) p->PIO_SODR = pin_mask;
		else                  p->PIO_CODR = pin_mask;
	}
}

static inline void set_dig_out_value(uint16_t v)
{
	static const uint32_t pins[] = {
		PHYCMD_DIGITAL_OUTPUT_0,  PHYCMD_DIGITAL_OUTPUT_1,
		PHYCMD_DIGITAL_OUTPUT_2,  PHYCMD_DIGITAL_OUTPUT_3,
		PHYCMD_DIGITAL_OUTPUT_4,  PHYCMD_DIGITAL_OUTPUT_5,
		PHYCMD_DIGITAL_OUTPUT_6,  PHYCMD_DIGITAL_OUTPUT_7,
		PHYCMD_DIGITAL_OUTPUT_8,  PHYCMD_DIGITAL_OUTPUT_9,
		PHYCMD_DIGITAL_OUTPUT_10, PHYCMD_DIGITAL_OUTPUT_11,
		PHYCMD_DIGITAL_OUTPUT_12, PHYCMD_DIGITAL_OUTPUT_13,
		PHYCMD_DIGITAL_OUTPUT_14, PHYCMD_DIGITAL_OUTPUT_15,
	};
	uint16_t reactive = waveform_reactive_dout_mask();
	for (int i = 0; i < 16; i++) {
		if (reactive & (1u << i)) continue;   /* owned by generator */
		if ((v >> i) & 1u)
			s_dig_out_ports[i]->PIO_SODR = 1u << (pins[i] & 0x1F);
		else
			s_dig_out_ports[i]->PIO_CODR = 1u << (pins[i] & 0x1F);
	}
}

static inline uint16_t get_dig_out_echo(void)
{
	uint16_t v = 0;
	static const uint32_t pins[] = {
		PHYCMD_DIGITAL_OUTPUT_0,  PHYCMD_DIGITAL_OUTPUT_1,
		PHYCMD_DIGITAL_OUTPUT_2,  PHYCMD_DIGITAL_OUTPUT_3,
		PHYCMD_DIGITAL_OUTPUT_4,  PHYCMD_DIGITAL_OUTPUT_5,
		PHYCMD_DIGITAL_OUTPUT_6,  PHYCMD_DIGITAL_OUTPUT_7,
		PHYCMD_DIGITAL_OUTPUT_8,  PHYCMD_DIGITAL_OUTPUT_9,
		PHYCMD_DIGITAL_OUTPUT_10, PHYCMD_DIGITAL_OUTPUT_11,
		PHYCMD_DIGITAL_OUTPUT_12, PHYCMD_DIGITAL_OUTPUT_13,
		PHYCMD_DIGITAL_OUTPUT_14, PHYCMD_DIGITAL_OUTPUT_15,
	};
	for (int i = 0; i < 16; i++)
		v |= ((s_dig_out_ports[i]->PIO_ODSR >> (pins[i] & 0x1F)) & 1u) << i;
	return v;
}

/* ============================================================
 *  ADC (8 channels, PDC/DMA, software-triggered at USB SOF)
 *
 *  Why SOF-triggered, not free-running:
 *    - Free-running mode lets the ADC re-convert at ~156 kHz per
 *      channel, but the phase of the conversion relative to the
 *      iso IN microframe rebuild wanders freely. For a lock-in
 *      amplifier or any FFT-adjacent analysis the host measures
 *      samples whose acquisition time drifts relative to the
 *      microframe boundary by up to ~6 µs per cycle.
 *    - SOF-triggered means every iso-IN frame carries an ADC
 *      snapshot acquired exactly one microframe earlier, with
 *      bounded phase jitter equal to the SOF delivery jitter
 *      (sub-microsecond on stock EHCI/xHCI hosts).
 *
 *  Behaviour:
 *    - adc_setup leaves the ADC armed but not converting. The PDC
 *      is pointed at g_adc_buf[0], fallback at g_adc_buf[1].
 *    - user_callback_sof_action (called from the USB SOF ISR at
 *      1 kHz FS / 8 kHz HS) writes ADC_CR_START. The PDC transfers
 *      8 × u16 into g_adc_buf[0] (~16 µs wall time). When the
 *      next SOF fires we start a fresh cycle into the same buffer;
 *      build_status_frame picks up whatever is there.
 *    - PDC ENDRX is not wired to an IRQ — we rely on the dual
 *      RPR/RNPR registers reloading automatically so the DMA is
 *      always pointed at valid RAM regardless of completion
 *      timing.
 *
 *  Boot-up: we trigger one manual conversion from adc_setup so
 *  g_adc_buf[0] holds real samples before the first iso IN packet
 *  rather than leftover zeros from power-on.
 * ============================================================ */

#define ADC_CHANNEL_NUM 12
uint16_t g_adc_buf[16][ADC_CHANNEL_NUM];
/* Index into g_adc_buf that the READER (build_status_frame) should
 * sample from. Incremented by the SOF handler after it swaps the PDC
 * target: the just-completed cycle's buffer is published, the other
 * one is now the write target. A lone u8 is atomic on ARMv7; both
 * sides just read/write it with volatile ordering.
 *
 * The two entries of the ring we actually use are g_adc_buf[0] and
 * g_adc_buf[1]; the rest of g_adc_buf[2..15] is legacy ring space
 * left in place so existing pointers/clients don't need to be
 * resized. */
static volatile uint8_t g_adc_publish_idx = 0;

/* Channels published in the 12-slot wire frame, indexed by SAM3X AD
 * channel number. Covers Due A0..A11 — full analog header on the Due,
 * matching the bench rig that now wires DAC0/DAC1/PWMs to A0..A11.
 * Mapping: Due Ax silkscreen → SAM3X AD channel:
 *   A0 -> AD7  (PA16)     A6 -> AD1  (PA3)
 *   A1 -> AD6  (PA24)     A7 -> AD0  (PA2)
 *   A2 -> AD5  (PA23)     A8 -> AD10 (PB17)
 *   A3 -> AD4  (PA22)     A9 -> AD11 (PB18)
 *   A4 -> AD3  (PA6)      A10 -> AD12 (PB19)
 *   A5 -> AD2  (PA4)      A11 -> AD13 (PB20)
 */
static const uint8_t g_adc_cdr_map[12] = {
	7, 6, 5, 4, 3, 2, 1, 0,    /* A0..A7 */
	10, 11, 12, 13,            /* A8..A11 */
};

static void adc_setup(void)
{
	pmc_enable_periph_clk(ID_ADC);

	/* Defensive: reset the one pad we know has a peripheral
	 * conflict pattern (PA16 / URXD1 / AD7). Aggressive bulk-PIO
	 * resets on the full ADC pad set broke A1 in testing, so leave
	 * the working pads alone — per §43.5.3 the ADC auto-reassigns
	 * the pin away from PIO when the channel is enabled. */
	pmc_enable_periph_clk(ID_PIOA);
	const uint32_t pa16_mask = 1u << 16;
	PIOA->PIO_PER  = pa16_mask;
	PIOA->PIO_ODR  = pa16_mask;
	PIOA->PIO_PUDR = pa16_mask;

	/* Shut down peripherals the bootloader might have left enabled
	 * on AD-adjacent pads, to keep unused output drivers quiet. */
	pmc_disable_periph_clk(ID_USART1);
	pmc_disable_periph_clk(ID_TWI1);
	pmc_disable_periph_clk(ID_SSC);
	pmc_disable_periph_clk(ID_CAN0);
	pmc_disable_periph_clk(ID_CAN1);

	/* ADC_STARTUP_SLOW gives the per-conversion startup machine
	 * ample time after wake-from-idle; at 8 kHz SOF-triggered
	 * single-shot we are effectively starting the ADC from idle
	 * every 125 µs. Without it the first one or two samples of
	 * each cycle are bogus. */
	adc_init(ADC, sysclk_get_peripheral_hz(), ADC_FREQ_MAX, ADC_STARTUP_NORM);
	adc_set_resolution(ADC, ADC_MR_LOWRES_BITS_12);

	/* Enable AD1..AD6 (= Due A6..A1) plus AD12 (= Due A10) and
	 * AD13 (= Due A11). On the bench chip AD7 (A0), AD10 (A8) and
	 * AD11 (A9) all read a stuck 0x800 regardless of the input
	 * voltage, so we skip those and pick 8 pins we can verify
	 * instead. AD0 (A7) is also left out because the user's current
	 * harness does not route anything to A7 — opening the AD12 /
	 * AD13 slots for the two PWM loopback wires they now have on
	 * A10 and A11.
	 *
	 * On a fresh chip where the upper-range channels work, restore
	 * the plain `for ch = 0..7` loop in both this block and in the
	 * adc_slot_map[] inside build_status_frame(). */
	for (int ch = 1; ch <= 6; ch++)
		adc_enable_channel(ADC, (enum adc_channel_num_t)ch);
	adc_enable_channel(ADC, (enum adc_channel_num_t)12);
	adc_enable_channel(ADC, (enum adc_channel_num_t)13);

	/* Per-channel tracking time: the sample-and-hold cap needs
	 * time to charge to the input voltage between successive
	 * channels. TRACKTIM=0 (default) is ~1 ADC cycle ≈ 50 ns at
	 * 20 MHz, which is FINE for a DAC driving a ~100 Ω on-chip
	 * output but UNUSABLE for a floating pin (MΩ source Z): the
	 * cap carries residual charge from the previous channel and
	 * every other sample looks noisy/biased.
	 *
	 * TRACKTIM=15 gives 16 ADC cycles = ~800 ns at 20 MHz — plenty
	 * for a 1 MΩ source Z. Cost is +~6 µs per 8-channel cycle
	 * (still well under the 125 µs microframe budget).
	 *
	 * SETTLING=3 (11 cycles) gives the sample-and-hold comparator
	 * enough time to stabilise after each channel switch; the
	 * ADC errata warns against SETTLING<2 with high-Z inputs. */
	uint32_t mr = ADC->ADC_MR;
	mr &= ~(ADC_MR_TRACKTIM_Msk | ADC_MR_SETTLING_Msk);
	mr |= ADC_MR_TRACKTIM(15);
	mr |= ADC_MR_SETTLING_AST17;
	ADC->ADC_MR = mr;

	ADC->ADC_EMR  = 0;

	/* FREE-RUN architecture: the ADC cycles through every enabled
	 * channel continuously and latches each result into the per-
	 * channel ADC_CDR[N] register. `build_status_frame` snapshots
	 * those registers when the host asks for a status frame — no
	 * PDC, no SOF trigger, no scan-order dependency.
	 *
	 * The earlier PDC + SOF-triggered path produced a reproducible
	 * "stuck at 0x800" symptom on any channel with index > 6 in the
	 * enabled set (AD7, AD10, AD11, AD13 seen). The root cause is
	 * that a PDC RCR=8 transfer with a sparse / non-contiguous
	 * channel-enable mask mis-sequences the last slot (probably
	 * races the EOC flags); direct CDR reads bypass it entirely
	 * and every channel converges on its real analog input within
	 * one free-run cycle (~20 µs for 16 channels). */
	/* Enable the channels g_adc_cdr_map[] reads: AD0..AD7 (Due A0..A7
	 * on Port A) plus AD10..AD13 (Due A8..A11 on Port B). 12 channels
	 * total. Mask = 0x3CFF.
	 *
	 * Skipped on purpose:
	 *   - AD8 (PB12) and AD9 (PB13): Due silkscreen labels A8/A9 are
	 *     wired to AD10/AD11, NOT AD8/AD9 — those AD inputs are not
	 *     exposed on the analog header.
	 *   - AD14 (PB21): clobbers DIN[15], see commit ba5fe1c.
	 *   - AD15 (PB15): would steal DAC0's pad. */
	ADC->ADC_CHER = 0x3CFFu;
	ADC->ADC_MR  |= ADC_MR_FREERUN;
	ADC->ADC_PTCR = ADC_PTCR_RXTDIS | ADC_PTCR_TXTDIS;
	ADC->ADC_IDR  = ~0u;
	ADC->ADC_CR   = ADC_CR_START;
	(void)g_adc_buf; (void)g_adc_publish_idx;
}

/* Called from the UDC SOF ISR (hooked via UDC_SOF_EVENT in
 * conf_usb.h). Runs at 8 kHz on HS.
 *
 * Double-buffer with atomic publish:
 *   Buffer layout: g_adc_buf[0] and g_adc_buf[1] alternate as
 *   "write target" and "reader target". At every SOF the cycle
 *   that was started at the previous SOF is guaranteed done
 *   (8 channels × ~1.5 µs = 12 µs << 125 µs), so we:
 *     1. flip g_adc_publish_idx to point at the just-completed
 *        buffer (what was the PDC write target); readers picking
 *        up the new publish_idx get fresh coherent data.
 *     2. point the PDC at the OTHER buffer for the new cycle.
 *     3. trigger START.
 *   Reader (build_status_frame) samples g_adc_publish_idx once
 *   and reads all 8 slots from that buffer. The writer never
 *   touches the currently-published buffer, so no torn reads.
 *
 * This fixes two bugs of the previous firmware:
 *   - PDC would drain after 16 samples and never re-arm; ADC
 *     values frozen at boot.
 *   - A naive "always rewrite RPR to g_adc_buf[0]" handler did
 *     re-arm but let the reader see g_adc_buf[0] mid-write,
 *     producing torn samples across the 8 channels.
 */
void user_callback_sof_action(void);
void user_callback_sof_action(void)
{
	/* No-op under FREE-RUN ADC. The ADC runs continuously and
	 * ADC_CDR[N] always holds the latest per-channel sample; we
	 * snapshot them on demand inside build_status_frame(). Kept as a
	 * defined symbol so conf_usb.h's UDC_SOF_EVENT hook still
	 * links. */
}

/* ============================================================
 *  DAC (2 channels, flexible selection)
 * ============================================================ */

static void dac_setup(void)
{
	/* PIO: DAC0 is on PB15, DAC1 on PB16 (both peripheral "X1").
	 * We have to release both pads from the PIO controller AND
	 * disable their weak pull-ups before the DACC can drive them
	 * as analog outputs. Without this step the pads stay under
	 * PIO control in their power-on state (input + pull-up) and
	 * the DACC writes are absorbed by the pull-up, producing a
	 * stuck near-3 V output on one channel and an inert pin on
	 * the other (depending on what the Arduino bootloader left
	 * behind). The matching "DAC1 works, DAC0 doesn't" pattern
	 * we chased for hours was exactly this. */
	pmc_enable_periph_clk(ID_PIOB);
	const uint32_t dac_mask = (1u << 15) | (1u << 16);
	PIOB->PIO_PUDR = dac_mask;   /* pull-up disable on PB15 + PB16 */
	PIOB->PIO_PDR  = dac_mask;   /* release to peripheral (extra function X1) */

	pmc_enable_periph_clk(ID_DACC);
	dacc_reset(DACC);
	dacc_set_writeprotect(DACC, 0);
	dacc_set_transfer_mode(DACC, 1);
	dacc_enable_flexible_selection(DACC);

	/* DACC_MR.REFRESH: "Refresh Period = 1024 * REFRESH / DACC Clock".
	 * With REFRESH=0 the chip's internal refresh is DISABLED and the
	 * analog output voltage starts decaying ~20 µs after the last
	 * conversion — the manual write path here lands every ~1 ms so
	 * the pin would sit mostly at its decayed idle value between
	 * writes. Set REFRESH=16 → 1024*16 / 42 MHz ≈ 390 µs which is
	 * fast enough to keep the voltage flat. (Datasheet §44.6.7.) */
	{
		uint32_t mr = DACC->DACC_MR;
		mr &= ~DACC_MR_REFRESH_Msk;
		mr |=  DACC_MR_REFRESH(16);
		DACC->DACC_MR = mr;
	}

	DACC->DACC_CHER = 3;  /* enable channels 0 and 1 */
}

static inline void dac_write_pair(uint16_t dac0, uint16_t dac1)
{
	if (dac0 > DAC_MAX) dac0 = DAC_MAX;
	if (dac1 > DAC_MAX) dac1 = DAC_MAX;
	dacc_write_conversion_data(DACC, dac0);
	DACC->DACC_CDR = dac1 | 0x1000u; /* channel 1 tag */
}

/* ============================================================
 *  process_command_frame() — called from udi_vendor.c ISR
 *
 *  Validates the 64-byte command in rx_buf, applies outputs,
 *  reads inputs, and fills the 64-byte status response in tx_buf.
 * ============================================================ */

/* Tracks whether the last command applied was valid. Bulk used to
 * compute this inline and immediately fold it into the status response;
 * with the iso path apply and build run on different EPs (and may even
 * run at different rates), so we cache the last-command outcome here. */
static volatile bool s_last_cmd_valid = true;

/* Last command handler's wall-clock execution time, in microseconds.
 * Reported to the host via status_msg_t.loop_time_us so the dashboard
 * can show what fraction of a 125-us microframe we spend inside the
 * apply path. Measured with the DWT cycle counter — enabled once at
 * boot via enable_dwt_cyccnt(). */
static volatile uint16_t s_last_loop_time_us = 0;

static inline uint32_t dwt_cyccnt(void)   { return DWT->CYCCNT; }

static void enable_dwt_cyccnt(void)
{
	/* Enable trace + DWT, then the cycle counter. SAM3X (Cortex-M3
	 * r2p0) doesn't expose the DWT_LAR unlock register, so we skip
	 * it — TRCENA + CYCCNTENA is enough for this core. */
	CoreDebug->DEMCR |= CoreDebug_DEMCR_TRCENA_Msk;
	DWT->CYCCNT = 0;
	DWT->CTRL |= DWT_CTRL_CYCCNTENA_Msk;
}

void apply_command_frame(const uint8_t *rx_buf)
{
	uint32_t t0 = dwt_cyccnt();
	const command_msg_t *cmd = (const command_msg_t *)rx_buf;

	bool cmd_valid = (cmd->header == COMMAND_HEADER) &&
	                 (crc16_ccitt(rx_buf, CRC_OVER_CMD_BYTES) == cmd->crc);

	if (!cmd_valid) {
		if (s_error_count < 0xFFFFu)
			s_error_count++;
		s_last_cmd_valid = false;
		return;
	}

	/* Apply digital outputs immediately */
	set_dig_out_value(cmd->digital_out);

	/* Apply DAC outputs — but only on channels currently in MANUAL.
	 * A channel in GENERATOR mode is being driven by the on-chip
	 * TC + DACC PDC chain in waveform.c; writing to its CDR here
	 * would race with the PDC and cause glitches.
	 *
	 * For MANUAL channels we also push the value into the waveform
	 * layer's per-channel hold, so if the OTHER channel is still
	 * generating (PDC active) the refill loop emits this value during
	 * the stopped-channel's slots instead of reverting to mid-rail.
	 * See waveform_set_manual_hold / refill_buffer SHAPE_OFF branch. */
	if (cmd->flags & FLAG_DAC_ENABLE) {
		uint16_t v0 = cmd->dac0, v1 = cmd->dac1;
		if (v0 > DAC_MAX) v0 = DAC_MAX;
		if (v1 > DAC_MAX) v1 = DAC_MAX;
		bool gen0 = waveform_dac_is_generating(0);
		bool gen1 = waveform_dac_is_generating(1);

		/* Always remember the latest manual setpoint so that a
		 * subsequent GEN → MANUAL transition lands at the correct
		 * value instead of snapping to mid-rail. The refill buffer
		 * for the OTHER (still-generator-driven) channel also uses
		 * this hold when producing an OFF-channel sample. */
		waveform_set_manual_hold(0, v0);
		waveform_set_manual_hold(1, v1);

		if (!gen0 && !gen1) {
			/* Both channels MANUAL, PDC idle. Pack v0+v1 into one
			 * 32-bit CDR write (DACC_MR.WORD=1 + TAG=1 in flexible
			 * selection mode, see dac_setup):
			 *   bits [11:0]  = sample1 value   bits [13:12] = CHTAG CH0
			 *   bits [27:16] = sample2 value   bits [29:28] = CHTAG CH1 */
			const uint32_t word =
			      ((uint32_t)(v0 & 0x0FFFu))
			    | (0u << 12)
			    | (((uint32_t)(v1 & 0x0FFFu)) << 16)
			    | (1u << 28);
			DACC->DACC_CDR = word;
		}
		/* When at least one channel is generator-driven we leave
		 * the DACC peripheral to the PDC: the refill_buffer loop
		 * in waveform.c reads waveform_set_manual_hold and emits
		 * the held value on every OFF slot, so the transition
		 * manual ⇌ generator stays glitch-free without us
		 * touching DACC_CDR from here. */
	}

	/* Apply manual PWM duty on pwm0 / pwm1 — symmetric with the DAC
	 * path above. The wire protocol carries only two manual PWM
	 * channels (pwm0, pwm1); the firmware supports four (pwm0..3)
	 * via the on-chip function generator vendor-SETUP plane
	 * (POST /api/fngen/play_builtin). A channel currently driven
	 * by the generator must not be stomped from here — the
	 * waveform_pwm_is_generating() guard returns true for exactly
	 * those slots. Duty = 0 on a manual-active channel parks the
	 * pin low and releases the PWM peripheral slot. */
	if (cmd->flags & FLAG_PWM_ENABLE) {
		if (!waveform_pwm_is_generating(0)) {
			if (cmd->pwm0 == 0) {
				waveform_stop_pwm_manual(0);
			} else {
				waveform_set_pwm_manual(0, cmd->pwm0);
			}
		}
		if (!waveform_pwm_is_generating(1)) {
			if (cmd->pwm1 == 0) {
				waveform_stop_pwm_manual(1);
			} else {
				waveform_set_pwm_manual(1, cmd->pwm1);
			}
		}
	}

	/* Sequence tracking */
	s_last_seq = cmd->seq_num;
	if (cmd->flags & FLAG_RESET_SEQ)
		s_last_seq = 0;

	s_last_cmd_valid = true;

	/* Measure handler duration. CYCCNT is 32-bit @ CPU clock; at 84 MHz
	 * wrap is every ~51 s which is way longer than any apply call. */
	uint32_t dt_cyc = dwt_cyccnt() - t0;
	uint32_t dt_us  = dt_cyc / 84u;           /* 84 MHz MCK on SAM3X */
	s_last_loop_time_us = dt_us > 0xFFFFu ? 0xFFFFu : (uint16_t)dt_us;

	/* seq_num echo and loop_time_us just changed — the next iso IN
	 * frame must reflect this, not the pre-command cached value. */
	s_status_dirty = true;
}

void build_status_frame(uint8_t *tx_buf)
{
	/* Fast path: a recent rebuild already produced a valid 64-byte
	 * frame in s_status_cached. Just copy it out. ~16 cycles @ 84 MHz
	 * = ~0.2 µs vs ~2 µs for a full rebuild+CRC. See header comment
	 * on s_status_cached for the dirty-flag protocol. */
	if (!s_status_dirty) {
		memcpy(tx_buf, s_status_cached, MSG_SIZE);
		return;
	}

	/* Clear dirty BEFORE the rebuild so a concurrent writer (any
	 * higher-priority ISR) can re-mark dirty and force the next
	 * call to rebuild. If we cleared after, and the writer fired
	 * mid-rebuild, the cache would go stale until the next
	 * unrelated dirty event. */
	s_status_dirty = false;

	status_msg_t *stat = (status_msg_t *)s_status_cached;

	/* Zero only the 64-byte protocol header. The iso path passes a
	 * 512-byte buffer with [64..511] pre-filled with zeros once at
	 * boot — re-zeroing the padding here would waste cycles every
	 * microframe. */
	memset(stat, 0, sizeof(*stat));

	stat->header      = STATUS_HEADER;
	stat->digital_in  = get_dig_in_value();
	stat->digital_out = get_dig_out_echo();

	/* ADC: snapshot the 12 channels listed in g_adc_cdr_map[] directly
	 * out of ADC_CDR[]. Mapping (stat->adc index → Due silkscreen pin):
	 *   adc[0]  → Due A0 (AD7)     adc[6]  → Due A6 (AD1)
	 *   adc[1]  → Due A1 (AD6)     adc[7]  → Due A7 (AD0)
	 *   adc[2]  → Due A2 (AD5)     adc[8]  → Due A8 (AD10)
	 *   adc[3]  → Due A3 (AD4)     adc[9]  → Due A9 (AD11)
	 *   adc[4]  → Due A4 (AD3)     adc[10] → Due A10 (AD12)
	 *   adc[5]  → Due A5 (AD2)     adc[11] → Due A11 (AD13) */
	/* ADC: snapshot ADC_CDR[] directly (FREE-RUN mode, see adc_setup).
	 * g_adc_cdr_map[] picks which AD channel lands in each wire slot. */
	for (int i = 0; i < ADC_CHANNEL_NUM; i++)
		stat->adc[i] = (uint16_t)(ADC->ADC_CDR[g_adc_cdr_map[i]] & 0x0FFFu);
	(void)g_adc_publish_idx; (void)g_adc_buf;

	/* Status flags */
	uint8_t sf = STATUS_USB_CONFIGURED;
	if (!s_last_cmd_valid) sf |= STATUS_ERROR_FLAG;
	stat->status_flags = sf;

	stat->seq_num       = s_last_seq;
	stat->loop_time_us  = s_last_loop_time_us;
	stat->uptime_ms     = s_uptime_ms;
	stat->error_count   = s_error_count;

	/* CRC over first 24 bytes of the status frame */
	stat->crc = crc16_ccitt(s_status_cached, CRC_OVER_STAT_BYTES);

	memcpy(tx_buf, s_status_cached, MSG_SIZE);
}

/* Bulk path keeps the original combined entry point — apply + build are
 * called back-to-back from a single bulk OUT callback, just like before. */
void process_command_frame(const uint8_t *rx_buf, uint8_t *tx_buf)
{
	apply_command_frame(rx_buf);
	build_status_frame(tx_buf);
}

/* ============================================================
 *  main()
 * ============================================================ */

int main(void)
{
	sysclk_init();
	irq_initialize_vectors();
	cpu_irq_enable();
	board_init();

	/* SysTick at 1 kHz for uptime_ms */
	SysTick_Config(sysclk_get_cpu_hz() / 1000);

	/* DWT cycle counter — used by apply_command_frame to fill
	 * status.loop_time_us. No-op on production runs since the
	 * counter just wraps freely. */
	enable_dwt_cyccnt();

	/* Configure the SAM3X watchdog. WDT_MR is write-once after reset
	 * so we have to set it up before anything else can touch it.
	 * Clock = SLCK / 128 = 32.768 kHz / 128 ≈ 256 Hz → one WDV tick
	 * ≈ 3.906 ms. WDV = 512 gives a ~2.0 s timeout; WDD = WDV means
	 * no "too-early" window (a kick at any time pets the dog).
	 * WDRSTEN asks the WDT to generate a processor-level reset on
	 * timeout, which the host-side IsoTransport auto-reset logic
	 * catches as a USB LIBUSB_ERROR_NO_DEVICE and retries opening
	 * the device. SysTick at 1 kHz pets the dog every 500 ms
	 * (WDT_KICK_EVERY_MS). */
	WDT->WDT_MR = WDT_MR_WDV(512) | WDT_MR_WDD(512) | WDT_MR_WDRSTEN;

	/* Initialise I/O peripherals */
	init_dig_in_ports();
	init_dig_out_ports();
	adc_setup();
	dac_setup();
	waveform_init();   /* must come after dac_setup — sets up TC0/PDC for DACC */

	/* Heartbeat LED on PB27 (Arduino Due D13 / on-board "L"). PIO
	 * output, pull-up disabled, starts LOW. Blink pattern is driven
	 * from SysTick_Handler. */
	pmc_enable_periph_clk(ID_PIOB);
	PIOB->PIO_PUDR = HEARTBEAT_PIN_MASK;
	PIOB->PIO_PER  = HEARTBEAT_PIN_MASK;
	PIOB->PIO_OER  = HEARTBEAT_PIN_MASK;
	PIOB->PIO_CODR = HEARTBEAT_PIN_MASK;

	/* Front-panel reset button on D3 (PC28). Configure as PIO input
	 * with internal pull-up so the SysTick poll above sees HIGH at
	 * idle and LOW only when the button is held down. */
	pmc_enable_periph_clk(ID_PIOC);
	PIOC->PIO_PER  = (1u << 28);
	PIOC->PIO_ODR  = (1u << 28);
	PIOC->PIO_PUER = (1u << 28);

	/* Start USB device stack */
	udc_start();

	/* Idle spin — open-loop protocol work happens in the UOTGHS ISR
	 * via vendor_bulk_out_cb → process_command_frame, and on the
	 * iso path via vendor_iso_out_cb. The reactive paths are also
	 * mostly ISR-driven (DACC_Handler ENDTX for waveform refill,
	 * SysTick for pulse cooldowns), with one remaining job here:
	 *
	 * polling DIN at high rate so reactive LUT (DIN-mask source) and
	 * PULSE_TRIG see edges within ~1 µs. We don't use PIO change
	 * interrupts in v1 — the polled path is simpler, doesn't risk
	 * priority-inverting the UOTGHS / DACC ISRs, and at the SAM3X's
	 * 84 MHz with no other CPU work happening between IRQs we get
	 * sub-microsecond latency anyway. */
	uint16_t prev_din = get_dig_in_value();
	for (;;) {
		uint16_t now_din = get_dig_in_value();
		if (now_din != prev_din) {
			waveform_on_din_change(now_din, prev_din);
			prev_din = now_din;
		}
	}
}
