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
	uint16_t adc[8];
	uint8_t  status_flags;
	uint8_t  seq_num;
	uint16_t crc;
	uint16_t loop_time_us;
	uint32_t uptime_ms;
	uint16_t error_count;
	uint8_t  reserved[30];
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

#define ADC_CHANNEL_NUM 8
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

static void adc_setup(void)
{
	pmc_enable_periph_clk(ID_ADC);

	/* Defensive: force PA16 (Due A0 / SAM3X AD7) to clean PIO input
	 * with no peripheral multiplexing and no pull-up. The Due
	 * bootloader may leave PA16 routed to a peripheral (it shares
	 * pads with USART1_SCK and PWML2) — if that peripheral is
	 * actively driving or pulling the line, the ADC reads a fixed
	 * non-analog level instead of the actual voltage. Known symptom
	 * on our bench: adc[7] locked at exactly 0x800 with zero
	 * variance regardless of input. The chip on `physical` still
	 * shows this after the reset (likely a real analog-mux fault
	 * on that specific part) but this init step is still correct
	 * and costs nothing. */
	pmc_enable_periph_clk(ID_PIOA);
	const uint32_t pa16_mask = 1u << 16;
	PIOA->PIO_PER  = pa16_mask;   /* PIO mode (take pad back from any peripheral) */
	PIOA->PIO_ODR  = pa16_mask;   /* input (not driving) */
	PIOA->PIO_PUDR = pa16_mask;   /* no pull-up — ADC needs the raw pin voltage */

	/* ADC_STARTUP_SLOW gives the per-conversion startup machine
	 * ample time after wake-from-idle; at 8 kHz SOF-triggered
	 * single-shot we are effectively starting the ADC from idle
	 * every 125 µs. Without it the first one or two samples of
	 * each cycle are bogus. */
	adc_init(ADC, sysclk_get_peripheral_hz(), ADC_FREQ_MAX, ADC_STARTUP_NORM);
	adc_set_resolution(ADC, ADC_MR_LOWRES_BITS_12);

	/* Enable AD0..AD6 (= Due A7..A1) and AD10 (= Due A8). AD7 is
	 * excluded: on the bench SAM3X we're validating against, the
	 * channel is stuck at exactly 0x800 regardless of input (chip-
	 * level analog-mux fault, documented in the adjacent ADC_EMR
	 * / ADC_CHDR block and in memory/adc_ch7_stuck_2048.md). AD10
	 * takes slot 7 in the PDC sequence so the status adc[] array
	 * covers Due A1..A8 instead of A0..A7, giving a contiguous
	 * block of 8 user-usable pins without a dead cell in the
	 * middle. On a fresh chip where AD7 works, revert the two
	 * changed lines below and the status frame goes back to
	 * covering Due A0..A7 unchanged. */
	for (int ch = 0; ch < 7; ch++)
		adc_enable_channel(ADC, (enum adc_channel_num_t)ch);
	adc_enable_channel(ADC, (enum adc_channel_num_t)10);

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

	/* Leave FREERUN cleared (adc_init default). Leaving TRGEN at 0
	 * means the only way to start a conversion is writing ADC_CR
	 * START — which user_callback_sof_action does on every SOF. */

	/* Force ADC_EMR = 0: keeps TAG=0 (so LCDR upper nibble stays
	 * zero and the PDC writes clean 12-bit samples to g_adc_buf)
	 * and CMPMODE=0 (no analog-compare IRQ). adc_init does not
	 * reset EMR; a warm start (USB bus-reset re-enumeration, WDT
	 * reset) can otherwise leave stale bits that corrupt the sample
	 * stream. Defensive, not known to fix any currently observed
	 * symptom. */
	ADC->ADC_EMR  = 0;

	/* Explicitly disable channels 7, 8, 9 and 11-15, leaving AD0..AD6
	 * plus AD10 enabled (mask = 0xFB80 in low 16 bits). Matches the
	 * CHER block above. */
	ADC->ADC_CHDR = 0xFFFFFB80u;

	/* Known chip-level limitation on the bench unit: AD7 (= Due A0 /
	 * PA16) reads exactly 0x800 with zero variance regardless of
	 * input. Verified both through the PDC and by reading ADC_CDR[7]
	 * directly; ADC_COR confirmed clear of any DIFF/OFF bit. Assumed
	 * to be a partial analog-mux fault on this specific SAM3X — we
	 * work around it by not enabling AD7 (see CHER block above).
	 *
	 * AD10 (= Due A8 / PB17) showed the same symptom in an earlier
	 * run; we re-enable it here regardless because the chip also
	 * might have been affected by an unrelated setup issue (EEVT
	 * leakage, wiring) and the protocol needs an 8th slot anyway.
	 * If on a given board AD10 also reads stuck, the callers see
	 * a fixed adc[7] value — no worse than the previous revision. */

	ADC->ADC_IDR  = ~(1u << 27);
	ADC->ADC_IER  = 1u << 27;

	/* PDC ring: primary + secondary. When the primary run completes
	 * the controller auto-loads the secondary; we re-point primary
	 * -> [0] and secondary -> [1] on every SOF so the DMA always
	 * has two full buffers queued. */
	ADC->ADC_RPR  = (uint32_t)g_adc_buf[0];
	ADC->ADC_RCR  = ADC_CHANNEL_NUM;
	ADC->ADC_RNPR = (uint32_t)g_adc_buf[1];
	ADC->ADC_RNCR = ADC_CHANNEL_NUM;
	ADC->ADC_PTCR = 1;     /* enable receive transfers */

	/* Bootstrap: one manual trigger so g_adc_buf[0] isn't all zeros
	 * before the first SOF arrives (e.g. during standalone debug
	 * with the USB cable unplugged). publish_idx starts at 0 so the
	 * reader returns g_adc_buf[0] even before any cycle completes. */
	g_adc_publish_idx = 0;
	ADC->ADC_CR   = ADC_CR_START;
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
	/* The buffer the PDC was just filling (the "previous" write
	 * target) is complete now — that was the one NOT currently
	 * published. Flip publish to it. */
	uint8_t old_publish = g_adc_publish_idx;
	uint8_t new_publish = old_publish ^ 1u;      /* just-completed buffer */
	uint8_t new_writer  = old_publish;           /* other buffer for new cycle */

	ADC->ADC_RPR  = (uint32_t)g_adc_buf[new_writer];
	ADC->ADC_RCR  = ADC_CHANNEL_NUM;
	ADC->ADC_RNCR = 0;

	/* Publish the just-completed data BEFORE triggering the new
	 * cycle — readers racing this ISR still see a consistent
	 * non-in-flight buffer. */
	g_adc_publish_idx = new_publish;

	ADC->ADC_CR = ADC_CR_START;
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
			/* Both channels in MANUAL mode and the PDC is idle.
			 *
			 * dac_setup configures DACC_MR.WORD=1 + TAG=1. In
			 * that mode the DACC interprets each 32-bit CDR
			 * write as TWO packed samples — not one. From the
			 * SAM3X ASF driver doc (dacc.c §enable_flexible):
			 *
			 *   "if the WORD field is set, the 2 bits DACC_CDR
			 *    [13:12] are used for channel selection of the
			 *    first data and the 2 bits DACC_CDR[29:28] for
			 *    channel selection of the second data."
			 *
			 * Layout:
			 *   bits  [11:0]  = sample1 value
			 *   bits [13:12] = sample1 CHTAG
			 *   bits [27:16] = sample2 value
			 *   bits [29:28] = sample2 CHTAG
			 *
			 * An earlier revision did two sequential single-
			 * sample writes with a TXRDY spin in between — that
			 * looked right on paper but in WORD=1 every write
			 * actually carries a phantom second sample. The
			 * first write sent (v0, CH0) + (0, CH0); the second
			 * sent (v1, CH1) + (0, CH0); net effect: CH0 stuck
			 * bouncing v0 ↔ 0 at the command rate while CH1
			 * (the "last written" channel) looked healthy. DAC0
			 * read ~0.94 V on a multimeter regardless of the
			 * commanded value; DAC1 tracked correctly. Packing
			 * both samples into one word makes the two-channel
			 * intent match what the DACC actually interprets. */
			const uint32_t word =
			      ((uint32_t)(v0 & 0x0FFFu))           /* sample 1 data */
			    | (0u << 12)                            /* sample 1 CHTAG = CH0 */
			    | (((uint32_t)(v1 & 0x0FFFu)) << 16)   /* sample 2 data */
			    | (1u << 28);                           /* sample 2 CHTAG = CH1 */
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

	/* ADC: read the currently-published buffer and reorder it so
	 * that `stat->adc[0]` is the reading on Arduino Due **A1** (the
	 * first working analog pin, since A0/AD7 is faulted on this
	 * chip) and `stat->adc[7]` is **A8**. Wire-protocol callers get
	 * a contiguous "first working → last working" sequence they can
	 * index from 0 without juggling the SAM3X AD-channel inversion.
	 *
	 * The PDC fills g_adc_buf[][k] with SAM3X AD channel k's last
	 * conversion (k runs 0..6 then 10, filling slots 0..7 in scan
	 * order). Due silkscreen A<n> corresponds to SAM3X AD(7-n) for
	 * n=1..7 and AD10 for n=8. So:
	 *    stat->adc[0] → Due A1 → AD6 → buf slot 6
	 *    stat->adc[1] → Due A2 → AD5 → buf slot 5
	 *    stat->adc[2] → Due A3 → AD4 → buf slot 4
	 *    stat->adc[3] → Due A4 → AD3 → buf slot 3
	 *    stat->adc[4] → Due A5 → AD2 → buf slot 2
	 *    stat->adc[5] → Due A6 → AD1 → buf slot 1
	 *    stat->adc[6] → Due A7 → AD0 → buf slot 0
	 *    stat->adc[7] → Due A8 → AD10 → buf slot 7 */
	uint8_t adc_idx = g_adc_publish_idx;
	static const uint8_t adc_slot_map[8] = { 6, 5, 4, 3, 2, 1, 0, 7 };
	for (int i = 0; i < ADC_CHANNEL_NUM; i++)
		stat->adc[i] = g_adc_buf[adc_idx][adc_slot_map[i]];

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
	 * output, active-low pull-up disabled, starts LOW. Blink pattern
	 * is driven from SysTick_Handler. */
	pmc_enable_periph_clk(ID_PIOB);
	PIOB->PIO_PUDR = HEARTBEAT_PIN_MASK;
	PIOB->PIO_PER  = HEARTBEAT_PIN_MASK;
	PIOB->PIO_OER  = HEARTBEAT_PIN_MASK;
	PIOB->PIO_CODR = HEARTBEAT_PIN_MASK;

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
