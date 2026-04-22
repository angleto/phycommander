/*
 * SPDX-License-Identifier: GPL-3.0-or-later
 * SPDX-FileCopyrightText: 2014-2026 Angelo Leto <angelo@leto.blue>
 */
/**
 * \file
 *
 * \brief PhyCommander on-chip function generator — implementation.
 *
 * Hardware resources used (SAM3X8E):
 *   * DACC (both channels, flexible selection + tag bit) — analog out
 *   * TC0 channel 0 — trigger source for the DAC conversions (PDC)
 *   * PDC TX of DACC — feeds the ping-pong buffer to the DAC
 *   * NVIC: DACC ENDTX interrupt — fires when one half of the
 *     ping-pong drains, schedules refill of the now-idle half
 *
 * Memory budget (Bilanciato profile — see PROTOCOL.md §4.4):
 *   * 2 × 1024 × 4 B = 8 KB ping-pong (samples interleaved across
 *     both DAC channels with the flexible-selection tag bit).
 *   * Per-channel arbitrary buffer: up to WAVE_MAX_ARB_SAMPLES × 2 B
 *     = 2 KB each.
 *   * 2 × sin LUT (1024 × 2 B = 2 KB) — shared between channels.
 *   Total ~14 KB of the 96 KB SAM3X SRAM.
 *
 * Wire / protocol contract is documented in `docs/firmware/PROTOCOL.md`.
 * Type definitions live in `waveform.h` and are mirrored on the host
 * side at `physerver/crates/phycmd-core/src/protocol/wave_types.rs`.
 */

#include "asf.h"
#include "waveform.h"
#include <math.h>
#include <string.h>

/* -------------------------------------------------------------------------
 *   Compile-time configuration
 * ------------------------------------------------------------------------- */

/* PINGPONG_SAMPLES picked to keep each refill burst short enough
 * that it can never starve the UOTGHS iso ISR (125 µs microframe).
 * 256 samples × ~0.25 µs-per-sample compute ≈ 60 µs refill burst —
 * UOTGHS can preempt mid-burst without losing iso packets. */
#define PINGPONG_SAMPLES   256u    /* per ping-pong half (interleaved both channels) */
#define SIN_LUT_SIZE       1024u   /* must be power of 2 for cheap masking */
#define SIN_LUT_MASK       (SIN_LUT_SIZE - 1u)

/* DACC tag bits used in flexible-selection mode (bit 12 of DACC_CDR
 * selects channel, see SAM3X8E datasheet §41.7.2). */
#define DACC_TAG_CH0       0x00000000u
#define DACC_TAG_CH1       0x00001000u
#define DACC_VAL_MASK      0x00000FFFu  /* 12-bit DAC */

/* TC0 ch0 used as the shared DAC trigger. Trigger source select for
 * DACC_MR.TRGSEL = 1 → TIO from TC0 channel 0 (datasheet §41.7.5). */
#define DACC_TRGSEL_TC0    1u

/* ID for ADC TC trigger when we move ADC off free-running mode. We
 * use TC0 channel 1; TRGSEL=1 in ADC_MR selects TIO from TC0 ch0,
 * TRGSEL=2 selects ch1 — see SAM3X8E §43.7.2. */
#define ADC_TRGSEL_TC0_CH1 2u

/* -------------------------------------------------------------------------
 *   Shared state (touched by the public API and the ENDTX ISR)
 *
 *   The ISR runs at NVIC priority 1 (just below UOTGHS), so the
 *   public API protects updates with a brief NVIC mask of DACC_IRQn
 *   only — never with global cpu_irq_disable.
 * ------------------------------------------------------------------------- */

/* Maximum simultaneous reactive instances per kind, see PROTOCOL.md §3.2 */
#define MAX_LUT_SLOTS         3
#define MAX_PULSE_TRIG_SLOTS  4

/* LUT input source descriptor cached per active LUT. */
typedef struct {
	uint8_t  input_src;          /* INPUT_SRC_ADC | INPUT_SRC_DIN_MASK */
	uint8_t  reserved;
	uint16_t input_arg;          /* ADC ch index or DIN bit mask */
	uint16_t n_entries;          /* power-of-2 (or smaller); index_mask = n_entries-1 */
	uint16_t output_mask;
	int16_t *entries;            /* points into one of s_lut_data slots */
	uint8_t  out_kind;           /* CHAN_KIND_DAC or CHAN_KIND_DOUT */
	uint8_t  out_idx;            /* DAC channel (0/1) or unused for DOUT */
} lut_state_t;

/* Per-channel THRESHOLD state. */
typedef struct {
	uint8_t  active;             /* 1 if this channel is in SHAPE_THRESHOLD */
	uint8_t  cur_high;           /* hysteresis state: last decision (1=high, 0=low) */
	struct WaveThresholdSpec spec;
	uint8_t  out_kind;
	uint8_t  out_idx;            /* DAC ch or DOUT bit */
} threshold_state_t;

/* Per-pulse PULSE_TRIG state. */
typedef struct {
	uint8_t  active;
	uint8_t  out_kind;
	uint8_t  out_idx;            /* DOUT bit */
	uint8_t  in_pulse;           /* 1 = currently in active phase */
	struct WavePulseSpec spec;
	uint32_t cooldown_remaining_us;  /* re-trigger blocked while > 0 */
	uint8_t  tc_block;           /* TC peripheral instance: 1 or 2 */
	uint8_t  tc_chan;            /* 0..2 */
} pulse_state_t;

/* Per-DAC-channel state. */
typedef struct {
	wave_shape_t shape;        /* SHAPE_OFF == channel is in MANUAL */
	uint8_t      flags;
	uint16_t     duty_x10;
	uint16_t     amplitude;
	uint16_t     offset;
	uint16_t     phase_offset_x16;
	uint32_t     freq_mHz;

	/* Phase accumulator in Q24.8 fixed point (24 bits whole + 8 frac).
	 * Wraps modulo 1.0 (0x100_0000). The increment per generated
	 * sample is recomputed whenever shape / freq_mHz / DAC clock
	 * change; keeping the accumulator across updates guarantees
	 * glitch-free FM/AM sweeps. */
	uint32_t     phase_q24_8;
	uint32_t     phase_inc_q24_8;

	/* Last 12-bit value applied to the DAC, held across PDC pumps.
	 * In SHAPE_OFF this is whatever the streaming Command frame last
	 * wrote (updated via waveform_set_manual_hold from main.c). In
	 * reactive modes (SHAPE_LUT/THRESHOLD/PID) this is the last value
	 * computed by the reactive ISR. refill_buffer() reads this to keep
	 * the DAC stable across PDC refills when the channel is not
	 * open-loop generating — without it the PDC would keep pumping
	 * `offset` at 200+ kSPS and drown any CPU write to DACC_CDR. */
	volatile uint16_t reactive_value;

	/* Arbitrary playback bookkeeping (only meaningful when
	 * shape == SHAPE_ARBITRARY). The samples themselves live in
	 * s_arb_buf[ch_idx][0..arb_n_samples-1]. */
	uint16_t     arb_n_samples;
	uint16_t     arb_loops_remaining;
	uint32_t     arb_sample_rate_hz;
	uint32_t     arb_step_q24_8;       /* increment per ping-pong sample */
	uint32_t     arb_pos_q24_8;        /* current position in samples */
} dac_chan_t;

static volatile dac_chan_t s_chan[WAVE_NUM_DAC];

/* Ping-pong buffer. PDC alternates between halves automatically via
 * its NextPointer / NextCounter mechanism; the ENDTX ISR reloads the
 * half that just drained while the other half is being clocked out. */
COMPILER_WORD_ALIGNED
static uint32_t s_pingpong[2][PINGPONG_SAMPLES];

/* Arbitrary-waveform sample buffers (uploaded via GEN_PLAY_ARBITRARY). */
COMPILER_WORD_ALIGNED
static int16_t s_arb_buf[WAVE_NUM_DAC][WAVE_MAX_ARB_SAMPLES];

/* Sin LUT: SIN_LUT_SIZE points covering one full cycle, range
 * [-INT16_MAX, +INT16_MAX]. Computed once at boot in waveform_init().
 * Linearly interpolated at runtime via the Q24.8 phase accumulator. */
COMPILER_WORD_ALIGNED
static int16_t s_sin_lut[SIN_LUT_SIZE];

/* Reactive-mode state pools.
 * LUT: 3 fixed slots — one per (DAC0, DAC1, DOUT) bound output.
 * Each owns its own 4096-entry × 2 B = 8 KB sample storage in
 * `s_lut_data`, totalling 24 KB SRAM. */
COMPILER_WORD_ALIGNED
static int16_t s_lut_data[MAX_LUT_SLOTS][WAVE_MAX_ARB_SAMPLES * 4 /* 4096 */];
static volatile lut_state_t s_lut[MAX_LUT_SLOTS];

/* THRESHOLD: per-DAC plus per-DOUT slot — 18 maximum, light state. */
static volatile threshold_state_t s_thr_dac [WAVE_NUM_DAC];
static volatile threshold_state_t s_thr_dout[WAVE_NUM_DOUT];

/* PULSE_TRIG: 4 simultaneous slots (TC1_CH0..2 + TC2_CH0). */
static volatile pulse_state_t s_pulse[MAX_PULSE_TRIG_SLOTS];

/* PWM channel state for MODE_PWM_DUTY. We expose 4 PWM channels
 * (out of SAM3X's 8) wired to Arduino Due pins 9, 8, 7, 6 — these
 * are PWMH4..PWMH7 on PIOC21..PIOC24 via peripheral B.
 *
 * PWM0 → Arduino Due pin 9  → PIOC21 / PWMH4
 * PWM1 → Arduino Due pin 8  → PIOC22 / PWMH5
 * PWM2 → Arduino Due pin 7  → PIOC23 / PWMH6
 * PWM3 → Arduino Due pin 6  → PIOC24 / PWMH7
 *
 * PWM4..PWM7 remain reserved in the protocol for future dead-time /
 * complementary / phase-shifted modes. */
typedef struct {
	uint8_t  active;          /* 1 iff this PWM channel is running */
	uint8_t  pwm_channel;     /* hardware PWM channel 4..7 (see map above) */
	uint8_t  pio_pin;         /* PIOC pin index 21..24 */
	uint32_t freq_mHz;
	uint16_t duty_x10;
} pwm_state_t;
#define WAVE_NUM_PWM_ACTIVE 4u
static volatile pwm_state_t s_pwm[WAVE_NUM_PWM_ACTIVE];
static uint8_t s_pwm_init_done = 0;

/* Forward declarations so waveform_stop_all() / play_builtin() above the
 * implementations can reach the PWM helpers. */
static bool pwm_hw_play(uint8_t idx, uint32_t freq_mHz, uint16_t duty_x10);
static void pwm_hw_stop(uint8_t idx);

/* PID: per-DAC slot. State in addition to the spec is integrator
 * accumulator, previous error (for derivative), and previous output
 * (for derivative LP filter). All in Q16.16 fixed-point. */
typedef struct {
	uint8_t  active;
	uint8_t  out_idx;            /* DAC channel 0 or 1 */
	struct WavePidSpec spec;
	int32_t  integral_q16_16;    /* anti-windup clamped to spec.integral_clamp */
	int32_t  prev_error_q16_16;
	int32_t  prev_d_q16_16;      /* low-pass filtered derivative */
} pid_state_t;
static volatile pid_state_t s_pid[WAVE_NUM_DAC];

/* Cache of the most recent DIN snapshot, used by the PIO change ISR
 * to compute LUT input indices and detect pulse-trig edges. Refreshed
 * by the ADC EOC ISR (which is the most frequent firmware ISR) and
 * by the PIO ISR itself. */
static volatile uint16_t s_last_din = 0;

/* Current shared DAC clock, ADC rate. Updated only from the public API,
 * read from the ISR. uint32_t access is atomic on Cortex-M3 (single
 * aligned store). */
static volatile uint32_t s_dac_clock_hz = WAVE_DEFAULT_DAC_CLOCK_HZ;
static volatile uint32_t s_adc_rate_hz  = WAVE_DEFAULT_DAC_CLOCK_HZ;

/* True iff DACC PDC + TC trigger are currently running. Set when we
 * transition from "all channels SHAPE_OFF" to "at least one active";
 * cleared on the reverse transition. */
static volatile bool s_pdc_running = false;

/* Forward declarations */
static void tc_dac_setup(uint32_t trigger_hz);
static void tc_dac_stop(void);
static void dacc_pdc_setup(void);
static void dacc_pdc_start(void);
static void dacc_pdc_stop(void);
static void refill_buffer(uint32_t *buf, uint32_t n_samples);
static void recompute_phase_increments(void);
static bool any_channel_active(void);
static bool channel_decode(uint16_t id, uint8_t *kind, uint8_t *idx);
void waveform_on_adc_endrx(void);

/* ADC sample ring (declared in main.c). */
#define ADC_CHANNEL_NUM 8
extern uint16_t g_adc_buf[16][ADC_CHANNEL_NUM];

/* -------------------------------------------------------------------------
 *   Sin LUT initialisation — done once at boot
 * ------------------------------------------------------------------------- */
static void sin_lut_init(void)
{
	for (uint32_t i = 0; i < SIN_LUT_SIZE; i++) {
		double t = (double)i / (double)SIN_LUT_SIZE;     /* 0..1 of cycle */
		double v = sin(2.0 * 3.14159265358979323846 * t);
		s_sin_lut[i] = (int16_t)(v * 32767.0);
	}
}

/* -------------------------------------------------------------------------
 *   TC0 channel 0 — drives DACC conversions
 *
 *   TC peripheral clock = MCK / 2 = 42 MHz. We use TC compare RC to
 *   reset the counter and toggle TIOA, giving a square wave at
 *   42_000_000 / (2 * RC) Hz. DACC is configured to trigger on the
 *   rising edge of TIOA → conversion rate = TC compare rate.
 *
 *   For 1 MSPS per channel (= 2 MHz DACC trigger because we alternate
 *   channels via tag bit), RC = 42_000_000 / (2 * 2_000_000) = 10.5 →
 *   round to 11 → effective 1.909 MHz trigger ≈ 954 kSPS per channel.
 *
 *   Since the requested rate is a target, we choose RC = round(TCCLK
 *   / (2 * 2 * requested_per_channel_hz)). The actual achieved rate
 *   is reported back via DAC_GET_CLOCK.
 * ------------------------------------------------------------------------- */

static uint32_t tc_dac_trigger_hz(void)
{
	/* Both DAC channels share the trigger via the tag-bit interleave,
	 * so the underlying TC must fire at 2 × per-channel rate. */
	return s_dac_clock_hz * 2u;
}

static void tc_dac_setup(uint32_t trigger_hz)
{
	pmc_enable_periph_clk(ID_TC0);

	/* TIMER_CLOCK1 = MCK / 2 = 42 MHz on the SAM3X8E.
	 * TIOA generates a 50%-duty square wave at trigger_hz so each
	 * rising edge produces exactly one DACC conversion. */
	uint32_t tc_clock = sysclk_get_main_hz() / 2u;
	uint32_t rc = tc_clock / trigger_hz;   /* counter wraps every RC ticks → trigger_hz */
	if (rc < 4u)        rc = 4u;
	if (rc > 0xFFFFu)   rc = 0xFFFFu;
	uint32_t ra = rc / 2u;                 /* TIOA goes HIGH at RA, LOW at RC */

	tc_init(TC0, 0,
	        TC_CMR_TCCLKS_TIMER_CLOCK1 |
	        TC_CMR_WAVE                 |
	        TC_CMR_WAVSEL_UP_RC         |
	        TC_CMR_ACPA_SET             |
	        TC_CMR_ACPC_CLEAR);

	tc_write_ra(TC0, 0, ra);
	tc_write_rc(TC0, 0, rc);
	tc_start(TC0, 0);
}

static void tc_dac_stop(void)
{
	tc_stop(TC0, 0);
}

/* -------------------------------------------------------------------------
 *   DACC PDC chain
 *
 *   Programmed once at boot in dacc_pdc_setup(). The ping-pong is
 *   driven by re-arming the NextPointer / NextCounter pair from the
 *   ENDTX ISR — when the "current" buffer half drains, the PDC
 *   automatically swaps to NextPointer and we get an interrupt to
 *   refill the half that just finished.
 * ------------------------------------------------------------------------- */

static void dacc_pdc_setup(void)
{
	/* Existing dac_setup() in main.c already enables the DACC clock,
	 * resets the peripheral, sets transfer mode 1 + flexible
	 * selection, and enables both channels. We only add the trigger
	 * configuration and PDC linkage here. */

	/* Trigger: TC0 ch0 TIOA on rising edge.
	 * DACC_MR_TRGEN is a single-bit field (no _Msk available); the
	 * value `DACC_MR_TRGEN_EN` already has the bit set. */
	uint32_t mr = DACC->DACC_MR;
	mr &= ~(DACC_MR_TRGSEL_Msk | DACC_MR_TRGEN);
	mr |=  DACC_MR_TRGSEL(DACC_TRGSEL_TC0) | DACC_MR_TRGEN_EN;
	DACC->DACC_MR = mr;

	/* Disable DACC PDC TX while we (re)program the pointers. */
	DACC->DACC_PTCR = DACC_PTCR_TXTDIS;
}

static void dacc_pdc_start(void)
{
	/* Pre-fill both ping-pong halves so the first ENDTX has fresh
	 * samples to play. */
	refill_buffer(s_pingpong[0], PINGPONG_SAMPLES);
	refill_buffer(s_pingpong[1], PINGPONG_SAMPLES);

	/* Current = half 0, Next = half 1. After the current drains the
	 * PDC swaps to next and fires ENDTX → ISR refills half 0 and
	 * sets it as the new Next. Steady-state oscillation between
	 * the two halves. */
	DACC->DACC_TPR  = (uint32_t)s_pingpong[0];
	DACC->DACC_TCR  = PINGPONG_SAMPLES;
	DACC->DACC_TNPR = (uint32_t)s_pingpong[1];
	DACC->DACC_TNCR = PINGPONG_SAMPLES;

	/* Enable ENDTX interrupt. Note: DACC_IER is write-1-to-set,
	 * other bits are unaffected. */
	DACC->DACC_IDR = ~0u;
	DACC->DACC_IER = DACC_IER_ENDTX;

	/* DACC priority 3: we do NOT touch UOTGHS priority (ASF sets it
	 * during udc_start() and changing it on older ASF breaks the
	 * iso scheduler — measured drop 8000→7000 Hz when we tried).
	 * Instead we park DACC low enough that whatever ASF picks for
	 * UOTGHS (typically 1 or 2), UOTGHS always preempts our 240 µs
	 * DACC refill ISR. ADC stays at 4 (below DACC). */
	NVIC_SetPriority(DACC_IRQn, 3);
	NVIC_ClearPendingIRQ(DACC_IRQn);
	NVIC_EnableIRQ(DACC_IRQn);

	/* Kick the PDC TX. From this moment the TC trigger drives DACC
	 * conversions and PDC pumps samples into DACC_CDR autonomously. */
	DACC->DACC_PTCR = DACC_PTCR_TXTEN;
}

static void dacc_pdc_stop(void)
{
	DACC->DACC_PTCR = DACC_PTCR_TXTDIS;
	DACC->DACC_IDR  = ~0u;
	NVIC_DisableIRQ(DACC_IRQn);
}

/**
 * \brief PDC ENDTX interrupt handler.
 *
 * Fires when DACC_TCR reaches zero (i.e. the half pointed to by TPR
 * has been fully transferred). The PDC automatically promoted
 * NextPointer/NextCounter to Pointer/Counter; we now have time
 * (typically PINGPONG_SAMPLES / trigger_hz seconds) to refill the
 * other half and arm it as the new Next.
 *
 * The "freshly drained" half is whichever buffer is NOT currently
 * being clocked out — TPR points at the now-active one, so the
 * other one is the one to refill. We track this with a static
 * toggle: at start half 0 is current, after the first ENDTX half 1
 * is current, and so on.
 */
static volatile uint8_t s_refill_idx = 0;   /* which half to refill next */

void DACC_Handler(void)
{
	uint32_t isr = DACC->DACC_ISR;

	if (isr & DACC_ISR_ENDTX) {
		uint32_t *fill = s_pingpong[s_refill_idx];
		refill_buffer(fill, PINGPONG_SAMPLES);

		/* Arm this half as the new Next. */
		DACC->DACC_TNPR = (uint32_t)fill;
		DACC->DACC_TNCR = PINGPONG_SAMPLES;

		s_refill_idx ^= 1u;
	}
}

/* -------------------------------------------------------------------------
 *   Sample compute — pure functions, evaluated per-sample by refill loop
 *
 *   Returns a signed value in roughly the range
 *   [-amplitude/2, +amplitude/2] before offset / clamp.  The unit is
 *   "raw DAC code" so caller does no scaling beyond + offset / clamp.
 * ------------------------------------------------------------------------- */

static inline int32_t shape_dc_unit(void) { return 0; }

static inline int32_t shape_sine_unit(uint32_t phase_q24_8)
{
	/* Phase 0..0x100_0000 maps to LUT index 0..SIN_LUT_SIZE.
	 * High 10 bits = LUT index, next 14 bits = inter-sample fraction. */
	uint32_t idx_int  = (phase_q24_8 >> 14) & SIN_LUT_MASK;
	uint32_t idx_frac = phase_q24_8 & 0x3FFFu;     /* 14-bit fraction */
	int32_t  a = s_sin_lut[idx_int];
	int32_t  b = s_sin_lut[(idx_int + 1u) & SIN_LUT_MASK];
	/* Linear interpolation: a + (b-a) * idx_frac/16384 */
	int32_t  d = b - a;
	return a + ((d * (int32_t)idx_frac) >> 14);
}

static inline int32_t shape_square_unit(uint32_t phase_q24_8, uint16_t duty_x10)
{
	/* duty_x10 in [0..1000]. Threshold = duty_x10 * 0x100_0000 / 1000. */
	uint32_t thr = ((uint64_t)duty_x10 * 0x01000000ULL) / 1000ULL;
	return (phase_q24_8 < thr) ? +32767 : -32767;
}

static inline int32_t shape_triangle_unit(uint32_t phase_q24_8)
{
	/* Symmetric triangle: 0..0.25 → 0..+1, 0.25..0.75 → +1..-1,
	 * 0.75..1 → -1..0.  Implemented as folded ramp. */
	int32_t  signed_ramp = (int32_t)(phase_q24_8 >> 8) - 0x8000;  /* -32768..+32767 */
	int32_t  abs_v = signed_ramp < 0 ? -signed_ramp : signed_ramp;
	return 32767 - 2 * abs_v;
}

static inline int32_t shape_sawtooth_unit(uint32_t phase_q24_8)
{
	/* Rising sawtooth: -1 at phase=0, +1 at phase→1. */
	return (int32_t)(phase_q24_8 >> 8) - 0x8000;
}

static inline int32_t shape_arbitrary_sample(uint8_t ch_idx,
                                             volatile dac_chan_t *c)
{
	uint32_t pos_int = (c->arb_pos_q24_8 >> 8) % c->arb_n_samples;
	int16_t  raw = s_arb_buf[ch_idx][pos_int];
	c->arb_pos_q24_8 += c->arb_step_q24_8;
	/* Loop termination: detect wrap. We track wraps in arb_loops_remaining
	 * which is decremented when arb_pos_q24_8 overflows past
	 * arb_n_samples × 256 (= one full pass). */
	if ((c->arb_pos_q24_8 >> 8) >= c->arb_n_samples) {
		c->arb_pos_q24_8 -= ((uint32_t)c->arb_n_samples << 8);
		if (c->arb_loops_remaining > 0) {
			c->arb_loops_remaining--;
			if (c->arb_loops_remaining == 0) {
				/* Loop count expired → auto-stop, channel back to MANUAL. */
				c->shape = SHAPE_OFF;
			}
		}
	}
	return raw;
}

/* Compute the DACC_CDR word for one channel at the given phase. */
static inline uint32_t channel_sample_word(uint8_t ch_idx, volatile dac_chan_t *c)
{
	uint32_t tag = (ch_idx == 0) ? DACC_TAG_CH0 : DACC_TAG_CH1;
	int32_t unit;     /* signed in approx [-32768, +32767] */
	switch (c->shape) {
	case SHAPE_OFF:      unit = 0; break;     /* shouldn't happen if we filter caller */
	case SHAPE_DC:       unit = shape_dc_unit(); break;
	case SHAPE_SINE:     unit = shape_sine_unit(c->phase_q24_8); break;
	case SHAPE_SQUARE:   unit = shape_square_unit(c->phase_q24_8, c->duty_x10); break;
	case SHAPE_TRIANGLE: unit = shape_triangle_unit(c->phase_q24_8); break;
	case SHAPE_SAWTOOTH: unit = shape_sawtooth_unit(c->phase_q24_8); break;
	case SHAPE_ARBITRARY: {
		int32_t raw = shape_arbitrary_sample(ch_idx, c);
		/* Arbitrary samples are int16 already in DAC-code-ish units;
		 * we still scale by amplitude and add offset for symmetry
		 * with built-ins.  amplitude=4095, offset=2048 = "no scale". */
		int32_t scaled = (raw * (int32_t)c->amplitude) >> 15;
		int32_t v = (int32_t)c->offset + scaled;
		if (v < 0) v = 0;
		if (v > (int32_t)DACC_VAL_MASK) v = DACC_VAL_MASK;
		return ((uint32_t)v & DACC_VAL_MASK) | tag;
	}
	/* Reactive modes: the reactive ISR (LUT/THRESHOLD/PID evaluation
	 * from SysTick, or PULSE_TRIG edge handler) updates c->reactive_value
	 * via waveform_set_reactive_dac. The PDC just pumps that value at
	 * the DAC clock rate, holding it stable between reactive updates.
	 * This replaces the pre-fix design where reactive_dac_write poked
	 * DACC_CDR directly and was overwritten within microseconds by the
	 * next PDC pump. */
	case SHAPE_LUT:
	case SHAPE_THRESHOLD:
	case SHAPE_PID:
		return ((uint32_t)c->reactive_value & DACC_VAL_MASK) | tag;
	default: unit = 0; break;
	}

	/* Common scale path for built-in shapes. unit is in
	 * [-32767, +32767]; we want amplitude/2 swing centred on offset. */
	int32_t scaled = ((int32_t)c->amplitude * unit) >> 16;     /* halved by /2 of amplitude */
	int32_t v = (int32_t)c->offset + scaled;
	if (v < 0) v = 0;
	if (v > (int32_t)DACC_VAL_MASK) v = DACC_VAL_MASK;
	return ((uint32_t)v & DACC_VAL_MASK) | tag;
}

/* -------------------------------------------------------------------------
 *   refill_buffer — called from ISR (DACC_Handler) and from
 *   dacc_pdc_start() to pre-fill at boot.
 *
 *   Layout: alternating CH0, CH1, CH0, CH1, ... so that the TC
 *   trigger fires at 2 × per-channel rate and DACC alternates
 *   conversions between the two channels via the tag bit.
 *
 *   For a half-buffer of N samples (N must be even), we emit N/2
 *   samples for each channel.
 * ------------------------------------------------------------------------- */

static void refill_buffer(uint32_t *buf, uint32_t n_samples)
{
	/* Snapshot per-channel state into local non-volatile structs so
	 * the inner loop can run without redundant volatile loads. */
	dac_chan_t c0 = s_chan[0];
	dac_chan_t c1 = s_chan[1];

	uint32_t inc0 = c0.phase_inc_q24_8;
	uint32_t inc1 = c1.phase_inc_q24_8;
	uint32_t ph0  = c0.phase_q24_8;
	uint32_t ph1  = c1.phase_q24_8;

	for (uint32_t i = 0; i < n_samples; i += 2u) {
		/* Even slots → CH0, odd slots → CH1. */
		if (c0.shape != SHAPE_OFF) {
			/* Update the working channel's phase in our local snapshot
			 * so SHAPE_ARBITRARY's auto-stop side-effects are visible
			 * to subsequent iterations. */
			c0.phase_q24_8 = ph0;
			buf[i] = channel_sample_word(0, &c0);
			ph0 += inc0;
		} else {
			/* Channel is in MANUAL. Emit the last value actually applied
			 * to the DAC (reactive_value, updated by apply_command_frame
			 * via waveform_set_manual_hold, or left at 2048 mid-rail
			 * before the first host write). Pre-fix this emitted
			 * `offset` and a transition GENERATOR → MANUAL silently
			 * snapped the DAC to mid-rail instead of holding the last
			 * manual value. */
			buf[i] = ((uint32_t)c0.reactive_value & DACC_VAL_MASK) | DACC_TAG_CH0;
		}

		if (c1.shape != SHAPE_OFF) {
			c1.phase_q24_8 = ph1;
			buf[i + 1] = channel_sample_word(1, &c1);
			ph1 += inc1;
		} else {
			buf[i + 1] = ((uint32_t)c1.reactive_value & DACC_VAL_MASK) | DACC_TAG_CH1;
		}
	}

	/* Persist updated phase + arbitrary bookkeeping back to the
	 * volatile shared state. */
	s_chan[0].phase_q24_8 = ph0;
	s_chan[1].phase_q24_8 = ph1;
	s_chan[0].arb_pos_q24_8     = c0.arb_pos_q24_8;
	s_chan[1].arb_pos_q24_8     = c1.arb_pos_q24_8;
	s_chan[0].arb_loops_remaining = c0.arb_loops_remaining;
	s_chan[1].arb_loops_remaining = c1.arb_loops_remaining;
	s_chan[0].shape = c0.shape;       /* may have flipped to OFF on loop end */
	s_chan[1].shape = c1.shape;
}

/* -------------------------------------------------------------------------
 *   Phase increment (re)computation
 *
 *   For built-ins: phase advances by freq_mHz / (sample_rate_hz × 1000)
 *     of a cycle per sample. In Q24.8 fixed point (where 1.0 cycle =
 *     0x100_0000), inc = freq_mHz × 0x100_0000 / (1000 × sample_rate_hz).
 *
 *   For arbitrary: arb_step_q24_8 is the increment in arb-buffer
 *     samples per generated sample, in Q24.8.
 * ------------------------------------------------------------------------- */

static void recompute_phase_increments_for(uint8_t i)
{
	dac_chan_t *c = (dac_chan_t *)&s_chan[i];
	uint32_t sr = s_dac_clock_hz;
	if (sr == 0) sr = 1;

	if (c->shape == SHAPE_ARBITRARY) {
		/* arb_step = arb_sample_rate_hz / DAC_clock per generated sample
		 * (in Q24.8 of "arb buffer indices"). */
		c->arb_step_q24_8 = (c->arb_sample_rate_hz == 0)
		                      ? 0
		                      : (uint32_t)(((uint64_t)c->arb_sample_rate_hz * 256ULL) / sr);
		c->phase_inc_q24_8 = 0;
	} else if (c->shape != SHAPE_OFF) {
		c->phase_inc_q24_8 = (uint32_t)(((uint64_t)c->freq_mHz * 0x01000000ULL)
		                                / ((uint64_t)1000ULL * sr));
	} else {
		c->phase_inc_q24_8 = 0;
	}
}

static void recompute_phase_increments(void)
{
	for (uint8_t i = 0; i < WAVE_NUM_DAC; i++)
		recompute_phase_increments_for(i);
}

static bool any_channel_active(void)
{
	for (uint8_t i = 0; i < WAVE_NUM_DAC; i++)
		if (s_chan[i].shape != SHAPE_OFF) return true;
	return false;
}

/* =========================================================================
 *   Reactive modes — LUT / THRESHOLD / PULSE_TRIG
 *
 *   Architecture:
 *     * ADC EOC ISR (chained off existing ADC PDC ENDRX) drives every
 *       reactive channel whose input is INPUT_SRC_ADC.
 *     * PIO change ISRs (one per PIO peripheral A/B/C/D) drive every
 *       reactive channel whose input is INPUT_SRC_DIN_MASK and every
 *       PULSE_TRIG channel waiting on a DIN edge.
 *     * SysTick (1 kHz) drives the PULSE_TRIG cooldown / duration
 *       countdown; the actual end-of-pulse is also TC-backed for
 *       sub-µs accuracy when needed (TC1/TC2 channels).
 *
 *   Output write helpers — care that we don't fight the DACC PDC.
 *   For DAC outputs in reactive mode, the channel's `shape` field is
 *   SHAPE_LUT/THRESHOLD/PID (NOT SHAPE_OFF), so the open-loop refill
 *   loop fills the buffer with zeros for that channel. We therefore
 *   need a separate fast path that bypasses PDC for reactive DAC
 *   updates. Simplest approach: when a reactive channel targets a
 *   DAC, we (a) keep `shape` = SHAPE_LUT/etc (so the streaming path
 *   ignores it) and (b) directly write DACC_CDR with the new value
 *   each time the reactive ISR fires.
 *
 *   This works because reactive update rates (≤ a few kHz) are much
 *   slower than the DACC FIFO drain rate, so direct CDR writes don't
 *   collide with anything. If a user combines a high-rate BUILTIN
 *   waveform AND a reactive LUT on the SAME DAC channel, the
 *   reactive write wins (because BUILTIN refill skips that channel
 *   when it's not SHAPE_*built-in). In practice users pick one mode
 *   per channel, so the conflict is academic.
 * ========================================================================= */

/* Forward declarations to keep the structural block tidy. */
extern void *s_dig_out_ports_ptr;   /* main.c: array of Pio* per DOUT bit */
extern uint8_t s_dig_out_pin_idx[]; /* main.c: per-DOUT-bit PIO pin index */

/* Stage a 12-bit reactive value on a DAC channel. The PDC refill loop
 * (`channel_sample_word` for SHAPE_LUT/THRESHOLD/PID) will pick it up
 * on the next buffer half and the DAC will hold it stable until the
 * next reactive update. Worst-case visibility latency is
 * PINGPONG_SAMPLES / (2 × dac_clock_per_channel) ≈ 640 µs at the
 * default 200 kSPS. We deliberately do NOT write DACC_CDR directly
 * here: the PDC is actively pumping and any CPU write to CDR would be
 * overwritten by the next PDC sample within ~2.5 µs. */
static inline void reactive_dac_write(uint8_t dac_idx, uint16_t v12)
{
	if (dac_idx >= WAVE_NUM_DAC) return;
	if (v12 > DACC_VAL_MASK) v12 = DACC_VAL_MASK;
	s_chan[dac_idx].reactive_value = v12;
}

/* Apply a DOUT bit-mask. The `mask` argument carries which bits to
 * change, the `bits` argument carries the new values for those bits.
 * Other DOUT bits stay at whatever the streaming Command frame last
 * wrote them. */
extern void reactive_dout_write(uint16_t mask, uint16_t bits);
/* (Implemented in main.c so it can use the existing s_dig_out_ports
 * + set_dig_out_value infrastructure without re-deriving the pin
 * mapping here.) */

/* ---- LUT support ----- */

static int8_t lut_slot_for(uint16_t channel_id)
{
	uint8_t kind, idx;
	if (!channel_decode(channel_id, &kind, &idx)) return -1;
	if (kind == CHAN_KIND_DAC) return (int8_t)idx;        /* slot 0 = DAC0, slot 1 = DAC1 */
	if (kind == CHAN_KIND_DOUT) return (int8_t)2;         /* shared slot 2 = DOUT */
	return -1;                                            /* PWM/DIN/ADC: no LUT support */
}

/* Compute the input index for a LUT given its input descriptor. */
static inline uint16_t lut_input_value(const lut_state_t *l)
{
	if (l->input_src == INPUT_SRC_ADC) {
		uint8_t ch = l->input_arg & 0x07;
		return (uint16_t)(g_adc_buf[0][ch] & 0x0FFF);     /* 12-bit ADC */
	}
	if (l->input_src == INPUT_SRC_DIN_MASK) {
		/* Pack the selected DIN bits into a contiguous index. The
		 * mask `input_arg` selects which DIN bits participate;
		 * each selected bit lands in the next-LSB slot of the index
		 * (LSB-first ordering). */
		uint16_t mask = l->input_arg;
		uint16_t din  = s_last_din;
		uint16_t out  = 0;
		uint8_t  bit  = 0;
		while (mask) {
			uint8_t b = (uint8_t)__builtin_ctz(mask);
			if ((din >> b) & 1u) out |= (1u << bit);
			bit++;
			mask &= mask - 1u;        /* clear lowest set bit */
		}
		return out;
	}
	return 0;
}

/* Apply one LUT slot. Called from PIO ISR (DIN-source) or ADC ISR
 * (ADC-source). */
static void lut_apply(uint8_t slot)
{
	const lut_state_t *l = (const lut_state_t *)&s_lut[slot];
	if (l->input_src == INPUT_SRC_NONE) return;

	uint16_t idx = lut_input_value(l);
	if (idx >= l->n_entries) idx = l->n_entries - 1u;
	int16_t v = l->entries[idx];

	if (l->out_kind == CHAN_KIND_DAC) {
		reactive_dac_write(l->out_idx, (uint16_t)v);
	} else if (l->out_kind == CHAN_KIND_DOUT) {
		reactive_dout_write(l->output_mask, (uint16_t)v);
	}
}

/* ---- THRESHOLD support ----- */

static inline uint16_t thr_input_value(const struct WaveThresholdSpec *spec)
{
	if (spec->input_src == INPUT_SRC_ADC) {
		uint8_t ch = spec->input_arg & 0x07;
		return (uint16_t)(g_adc_buf[0][ch] & 0x0FFF);
	}
	if (spec->input_src == INPUT_SRC_DIN_MASK) {
		/* For threshold on DIN: collapse selected bits to "any of
		 * them is high" semantics (mask AND). Numerically the input
		 * is the popcount × 4095 / popcount = 0 or 4095. Quick
		 * approximation: if any selected bit is high → 4095. */
		return (s_last_din & spec->input_arg) ? 4095u : 0u;
	}
	return 0;
}

static void threshold_eval(uint8_t kind, uint8_t idx, volatile threshold_state_t *t)
{
	if (!t->active) return;
	struct WaveThresholdSpec spec_copy = t->spec;     /* drop volatile for inner read */
	uint16_t v = thr_input_value(&spec_copy);
	uint8_t  was_high = t->cur_high;
	uint8_t  now_high = was_high;
	if (v > t->spec.thr_high)      now_high = 1;
	else if (v < t->spec.thr_low)  now_high = 0;
	if (now_high != was_high) {
		t->cur_high = now_high;
		uint16_t outv = now_high ? t->spec.val_high : t->spec.val_low;
		if (kind == CHAN_KIND_DAC) {
			reactive_dac_write(idx, outv);
		} else if (kind == CHAN_KIND_DOUT) {
			reactive_dout_write(1u << idx, outv ? (1u << idx) : 0u);
		}
	}
}

/* ---- PULSE_TRIG support ----- */

static void pulse_trigger_start(volatile pulse_state_t *p)
{
	if (p->cooldown_remaining_us > 0) return;   /* still cooling down */
	if (p->in_pulse) return;                    /* already pulsing */
	p->in_pulse = 1;
	p->cooldown_remaining_us = p->spec.cooldown_us + p->spec.duration_us;
	if (p->out_kind == CHAN_KIND_DOUT) {
		uint16_t mask = 1u << p->out_idx;
		uint16_t bits = p->spec.active_level ? mask : 0;
		reactive_dout_write(mask, bits);
	}
	/* Pulse end is handled by the SysTick countdown (1 kHz tick).
	 * For sub-ms pulse widths a TC compare can be wired here, but
	 * v1 keeps it simple at 1 ms granularity. */
}

static bool any_adc_consumer_active(void);
void waveform_on_adc_endrx(void);

static void pulse_tick_1ms(void)
{
	for (uint8_t i = 0; i < MAX_PULSE_TRIG_SLOTS; i++) {
		volatile pulse_state_t *p = &s_pulse[i];
		if (!p->active) continue;
		if (p->cooldown_remaining_us > 1000)
			p->cooldown_remaining_us -= 1000;
		else
			p->cooldown_remaining_us = 0;
		/* End-of-pulse: when remaining time drops below cooldown_us
		 * we've crossed from "in pulse" to "in cooldown". */
		if (p->in_pulse && p->cooldown_remaining_us <= p->spec.cooldown_us) {
			p->in_pulse = 0;
			if (p->out_kind == CHAN_KIND_DOUT) {
				uint16_t mask = 1u << p->out_idx;
				uint16_t bits = p->spec.active_level ? 0 : mask;
				reactive_dout_write(mask, bits);
			}
		}
	}
}

/* Called from SysTick handler in main.c (see hookup below).
 *
 * Reactive ADC-driven modes (LUT/THRESHOLD/PID) are evaluated here at
 * 1 kHz instead of inside ADC_Handler. Running them from the ADC ISR
 * at ~60 kHz ENDRX rate was starving UOTGHS microframes and hanging
 * the USB stack; 1 kHz is plenty responsive for any practical analog
 * control loop and keeps the ADC peripheral free-running purely for
 * g_adc_buf snapshot purposes (no NVIC hit). */
void waveform_systick_1ms(void)
{
	pulse_tick_1ms();
	if (any_adc_consumer_active()) {
		waveform_on_adc_endrx();
	}
}

/* ---- PID closed-loop step ----
 *
 * Q16.16 fixed-point throughout. Evaluated once per ADC EOC (so the
 * effective PID rate equals the ADC sampling rate, which can be
 * configured via ADC_SET_RATE). The derivative term is single-pole
 * low-passed with a fixed alpha=0.25 to suppress measurement noise. */
static void pid_step(uint8_t dac_idx)
{
	volatile pid_state_t *p = &s_pid[dac_idx];
	if (!p->active) return;
	if (p->spec.input_src != INPUT_SRC_ADC) return;
	uint8_t adc_ch = p->spec.input_arg & 0x07;
	int32_t input = (int32_t)(g_adc_buf[0][adc_ch] & 0x0FFF);

	int32_t err   = (p->spec.setpoint - input) << 16;     /* Q16.16 */
	/* Integrator (Ki·∫err·dt where dt = 1/ADC_rate, baked into Ki). */
	int64_t i_acc = (int64_t)p->integral_q16_16 + (int64_t)err;
	int32_t clamp = p->spec.integral_clamp;
	if (i_acc >  (int64_t)clamp) i_acc = clamp;
	if (i_acc < -(int64_t)clamp) i_acc = -clamp;
	p->integral_q16_16 = (int32_t)i_acc;

	/* Derivative: (err - prev_err) low-pass filtered. alpha=1/4. */
	int32_t d_raw = err - p->prev_error_q16_16;
	int32_t d_filt = p->prev_d_q16_16 + ((d_raw - p->prev_d_q16_16) >> 2);
	p->prev_d_q16_16 = d_filt;
	p->prev_error_q16_16 = err;

	/* output = Kp·err + Ki·integral + Kd·derivative, all Q16.16
	 * multiplications, shift down to integer DAC units. */
	int64_t out64 = ((int64_t)p->spec.kp_q16_16 * err) >> 16;
	out64       += ((int64_t)p->spec.ki_q16_16 * p->integral_q16_16) >> 16;
	out64       += ((int64_t)p->spec.kd_q16_16 * d_filt) >> 16;
	int32_t out_int = (int32_t)(out64 >> 16);    /* now in DAC units */

	if (out_int < (int32_t)p->spec.out_min) out_int = p->spec.out_min;
	if (out_int > (int32_t)p->spec.out_max) out_int = p->spec.out_max;
	reactive_dac_write(dac_idx, (uint16_t)out_int);
}

/* ---- ADC EOC ISR (chained from existing ADC PDC ring) ----
 *
 * Called by main.c's existing ADC handler on every ENDRX (fresh
 * sample sweep available). Iterates active reactive channels whose
 * input is INPUT_SRC_ADC and updates their outputs. */
void waveform_on_adc_endrx(void)
{
	/* Refresh DIN snapshot here too — saves a separate poll path
	 * and keeps it close to the conversion that produced the
	 * sample we'll use for any DIN-mask LUT below. */
	extern uint16_t get_dig_in_value(void);
	s_last_din = get_dig_in_value();

	/* LUTs */
	for (uint8_t s = 0; s < MAX_LUT_SLOTS; s++) {
		if (s_lut[s].input_src == INPUT_SRC_ADC) lut_apply(s);
	}
	/* THRESHOLDs */
	for (uint8_t i = 0; i < WAVE_NUM_DAC; i++) {
		if (s_thr_dac[i].active && s_thr_dac[i].spec.input_src == INPUT_SRC_ADC)
			threshold_eval(CHAN_KIND_DAC, i, &s_thr_dac[i]);
	}
	for (uint8_t i = 0; i < WAVE_NUM_DOUT; i++) {
		if (s_thr_dout[i].active && s_thr_dout[i].spec.input_src == INPUT_SRC_ADC)
			threshold_eval(CHAN_KIND_DOUT, i, &s_thr_dout[i]);
	}
	/* PIDs */
	for (uint8_t i = 0; i < WAVE_NUM_DAC; i++) {
		if (s_pid[i].active) pid_step(i);
	}
}

/* No ADC_Handler: reactive ADC consumers run from waveform_systick_1ms
 * at 1 kHz instead of from the ADC ENDRX ISR. Running per-sweep at
 * ~60 kHz was saturating the UOTGHS iso scheduler; 1 kHz is enough for
 * any practical analog control loop and keeps the ADC peripheral
 * free-running purely for g_adc_buf snapshots. */

/* Called by main.c on any DIN change (the polled main loop reaches
 * here every time get_dig_in_value() returns a different value). */
void waveform_on_din_change(uint16_t new_din, uint16_t prev_din)
{
	s_last_din = new_din;
	uint16_t changed = new_din ^ prev_din;

	/* Pulse-trig: edge detection per slot */
	for (uint8_t i = 0; i < MAX_PULSE_TRIG_SLOTS; i++) {
		volatile pulse_state_t *p = &s_pulse[i];
		if (!p->active) continue;
		uint16_t bit = 1u << p->spec.input_din_bit;
		if (!(changed & bit)) continue;
		uint8_t rising = (new_din & bit) != 0;
		bool fire = false;
		if (p->spec.edge == PULSE_EDGE_RISING  &&  rising) fire = true;
		if (p->spec.edge == PULSE_EDGE_FALLING && !rising) fire = true;
		if (p->spec.edge == PULSE_EDGE_ANY) fire = true;
		if (fire) pulse_trigger_start(p);
	}

	/* LUT (DIN-mask source) */
	for (uint8_t s = 0; s < MAX_LUT_SLOTS; s++) {
		if (s_lut[s].input_src == INPUT_SRC_DIN_MASK) {
			if ((changed & s_lut[s].input_arg) != 0) lut_apply(s);
		}
	}

	/* THRESHOLD (DIN-mask source) — uncommon but supported */
	for (uint8_t i = 0; i < WAVE_NUM_DAC; i++) {
		if (s_thr_dac[i].active && s_thr_dac[i].spec.input_src == INPUT_SRC_DIN_MASK)
			threshold_eval(CHAN_KIND_DAC, i, &s_thr_dac[i]);
	}
}

/* Bring the PDC up if a channel just went GENERATOR; tear it down if
 * the last GENERATOR channel just went OFF. Idempotent. */
static void update_pdc_running(void)
{
	bool want = any_channel_active();
	if (want && !s_pdc_running) {
		tc_dac_setup(tc_dac_trigger_hz());
		dacc_pdc_setup();
		s_refill_idx = 0;
		dacc_pdc_start();
		s_pdc_running = true;
	} else if (!want && s_pdc_running) {
		dacc_pdc_stop();
		tc_dac_stop();
		s_pdc_running = false;
	}
}

/* =========================================================================
 *   Public API
 * ========================================================================= */

void waveform_init(void)
{
	memset((void *)s_chan, 0, sizeof(s_chan));
	for (uint8_t i = 0; i < WAVE_NUM_DAC; i++) {
		s_chan[i].shape = SHAPE_OFF;
		s_chan[i].amplitude = 4095;
		s_chan[i].offset    = 2048;
		s_chan[i].duty_x10  = 500;
		/* Hold value for MANUAL and reactive-mode refills. 2048 is
		 * mid-rail of the 0..4095 DAC range, matches the silent-DAC
		 * default until the host writes. */
		s_chan[i].reactive_value = 2048;
	}
	s_dac_clock_hz = WAVE_DEFAULT_DAC_CLOCK_HZ;
	s_adc_rate_hz  = WAVE_DEFAULT_DAC_CLOCK_HZ;
	s_pdc_running  = false;

	/* Reset reactive state */
	memset((void *)s_lut,      0, sizeof(s_lut));
	memset((void *)s_thr_dac,  0, sizeof(s_thr_dac));
	memset((void *)s_thr_dout, 0, sizeof(s_thr_dout));
	memset((void *)s_pulse,    0, sizeof(s_pulse));
	memset((void *)s_pid,      0, sizeof(s_pid));
	s_last_din = 0;

	sin_lut_init();

	/* ADC_IRQn is never enabled: reactive ADC-driven modes are
	 * evaluated from waveform_systick_1ms() at 1 kHz. Leaving the
	 * ADC line armed would raise ENDRX at ~75 kHz in free-running
	 * mode and saturate the UOTGHS iso scheduler (historical bug,
	 * see commit history). The ADC PDC ring is kept running purely
	 * so g_adc_buf carries a fresh snapshot for protocol status
	 * frames and reactive sysick evaluation. */
}

/* Scan per-channel state; return true iff any reactive slot is
 * subscribed to an ADC EOC event. Called from every mode-change
 * entry point and the idempotent toggle below. */
static bool any_adc_consumer_active(void)
{
	for (uint8_t s = 0; s < MAX_LUT_SLOTS; s++)
		if (s_lut[s].input_src == INPUT_SRC_ADC) return true;
	for (uint8_t i = 0; i < WAVE_NUM_DAC; i++)
		if (s_thr_dac[i].active && s_thr_dac[i].spec.input_src == INPUT_SRC_ADC) return true;
	for (uint8_t i = 0; i < WAVE_NUM_DOUT; i++)
		if (s_thr_dout[i].active && s_thr_dout[i].spec.input_src == INPUT_SRC_ADC) return true;
	for (uint8_t i = 0; i < WAVE_NUM_DAC; i++)
		if (s_pid[i].active) return true;   /* v3 PID is ADC-only */
	return false;
}

void waveform_stop_all(void)
{
	NVIC_DisableIRQ(DACC_IRQn);
	for (uint8_t i = 0; i < WAVE_NUM_DAC; i++) {
		s_chan[i].shape = SHAPE_OFF;
		s_thr_dac[i].active = 0;
		s_pid[i].active = 0;
	}
	for (uint8_t i = 0; i < WAVE_NUM_DOUT; i++) {
		s_thr_dout[i].active = 0;
	}
	for (uint8_t s = 0; s < MAX_LUT_SLOTS; s++) {
		s_lut[s].input_src = INPUT_SRC_NONE;
	}
	for (uint8_t i = 0; i < MAX_PULSE_TRIG_SLOTS; i++) {
		s_pulse[i].active = 0;
	}
	for (uint8_t i = 0; i < WAVE_NUM_PWM_ACTIVE; i++) {
		pwm_hw_stop(i);
	}
	update_pdc_running();
}

bool waveform_dac_is_generating(uint8_t dac_idx)
{
	if (dac_idx >= WAVE_NUM_DAC) return false;
	return s_chan[dac_idx].shape != SHAPE_OFF;
}

void waveform_set_manual_hold(uint8_t dac_idx, uint16_t v12)
{
	if (dac_idx >= WAVE_NUM_DAC) return;
	if (v12 > DACC_VAL_MASK) v12 = DACC_VAL_MASK;
	s_chan[dac_idx].reactive_value = v12;
}

uint16_t waveform_reactive_dout_mask(void)
{
	uint16_t mask = 0;
	for (uint8_t i = 0; i < WAVE_NUM_DOUT; i++) {
		if (s_thr_dout[i].active) mask |= 1u << i;
	}
	for (uint8_t s = 0; s < MAX_PULSE_TRIG_SLOTS; s++) {
		if (s_pulse[s].active && s_pulse[s].out_kind == CHAN_KIND_DOUT)
			mask |= 1u << s_pulse[s].out_idx;
	}
	for (uint8_t s = 0; s < MAX_LUT_SLOTS; s++) {
		if (s_lut[s].input_src != INPUT_SRC_NONE && s_lut[s].out_kind == CHAN_KIND_DOUT)
			mask |= s_lut[s].output_mask;
	}
	return mask;
}

/* =========================================================================
 *   PWM peripheral — MODE_PWM_DUTY on PWM channels 0..3
 *
 *   Lazy-initialised on the first play_builtin for a PWM channel.
 *   CLKA prescaler is picked dynamically per channel so each user
 *   always gets at least ~10 bits of duty resolution at the requested
 *   frequency. PWMH4..7 are routed to PC21..PC24 (Arduino Due pins 9,
 *   8, 7, 6) via peripheral B.
 * ========================================================================= */

static void pwm_hw_init(void)
{
	if (s_pwm_init_done) return;

	/* Route PC21..PC24 to PWM peripheral B. Disable PIO control
	 * (give the pin to the peripheral) and select peripheral B. */
	pmc_enable_periph_clk(ID_PIOC);
	uint32_t mask = (1u << 21) | (1u << 22) | (1u << 23) | (1u << 24);
	PIOC->PIO_PDR  = mask;                /* pin release to peripheral */
	PIOC->PIO_ABSR |= mask;               /* B peripheral select       */
	PIOC->PIO_PUDR = mask;                /* no pull-up                */

	/* Enable PWM peripheral clock. */
	pmc_enable_periph_clk(ID_PWM);

	/* Initialise with CLKA = MCK (no prescaling). Per-channel CMR
	 * is configured later with a dynamic CPRE. CLKB left unused. */
	pwm_clock_t clock_cfg = {
		.ul_clka = sysclk_get_main_hz(),
		.ul_clkb = 0,
		.ul_mck  = sysclk_get_main_hz(),
	};
	pwm_init(PWM, &clock_cfg);

	/* Populate channel map for PWM0..3. */
	for (uint8_t i = 0; i < WAVE_NUM_PWM_ACTIVE; i++) {
		s_pwm[i].pwm_channel = 4 + i;     /* PWMH4..7 */
		s_pwm[i].pio_pin     = 21 + i;    /* PIOC21..24 */
		s_pwm[i].active      = 0;
	}

	s_pwm_init_done = 1;
}

/* Pick a prescaler (PREA) that keeps the period in [256, 65535] —
 * always at least 8 bits of duty resolution — and a CPRD that gives
 * the requested frequency. Returns actual CPRD + CPRE bits. */
static void pwm_hw_pick_clock(uint32_t freq_hz, uint32_t *out_cpre, uint32_t *out_cprd)
{
	uint32_t mck = sysclk_get_main_hz();
	if (freq_hz == 0) freq_hz = 1;
	uint32_t cpre;
	uint32_t cprd;
	for (cpre = 0; cpre < 11; cpre++) {
		uint32_t clk = mck >> cpre;
		cprd = clk / freq_hz;
		if (cprd <= 65535) break;
	}
	if (cprd < 4) cprd = 4;
	if (cprd > 65535) cprd = 65535;
	*out_cpre = cpre;
	*out_cprd = cprd;
}

static bool pwm_hw_play(uint8_t idx, uint32_t freq_mHz, uint16_t duty_x10)
{
	if (idx >= WAVE_NUM_PWM_ACTIVE) return false;
	pwm_hw_init();

	uint32_t freq_hz = (freq_mHz + 500u) / 1000u;
	if (freq_hz == 0) freq_hz = 1;

	uint32_t cpre, cprd;
	pwm_hw_pick_clock(freq_hz, &cpre, &cprd);
	uint32_t cdty = (cprd * duty_x10) / 1000u;
	if (cdty > cprd) cdty = cprd;

	uint32_t ch = s_pwm[idx].pwm_channel;

	/* Disable before reconfig (allows period change without glitches
	 * on first start; for parameter updates we also write the "update"
	 * registers so the change is picked up at the next period boundary
	 * without disabling. */
	if (!s_pwm[idx].active) {
		pwm_channel_disable(PWM, 1u << ch);
		PWM->PWM_CH_NUM[ch].PWM_CMR = (cpre & PWM_CMR_CPRE_Msk) | PWM_CMR_CPOL;
		PWM->PWM_CH_NUM[ch].PWM_CPRD = cprd;
		PWM->PWM_CH_NUM[ch].PWM_CDTY = cdty;
		pwm_channel_enable(PWM, 1u << ch);
	} else {
		/* Live update: use the update registers so PWM applies the
		 * new CPRD/CDTY synchronously at the next period boundary. */
		PWM->PWM_CH_NUM[ch].PWM_CPRDUPD = cprd;
		PWM->PWM_CH_NUM[ch].PWM_CDTYUPD = cdty;
		/* If cpre changed we need a full restart — rare, but handle it. */
		uint32_t cur_cpre = PWM->PWM_CH_NUM[ch].PWM_CMR & PWM_CMR_CPRE_Msk;
		if (cur_cpre != cpre) {
			pwm_channel_disable(PWM, 1u << ch);
			PWM->PWM_CH_NUM[ch].PWM_CMR = (cpre & PWM_CMR_CPRE_Msk) | PWM_CMR_CPOL;
			PWM->PWM_CH_NUM[ch].PWM_CPRD = cprd;
			PWM->PWM_CH_NUM[ch].PWM_CDTY = cdty;
			pwm_channel_enable(PWM, 1u << ch);
		}
	}

	s_pwm[idx].freq_mHz = freq_mHz;
	s_pwm[idx].duty_x10 = duty_x10;
	s_pwm[idx].active   = 1;
	return true;
}

static void pwm_hw_stop(uint8_t idx)
{
	if (idx >= WAVE_NUM_PWM_ACTIVE) return;
	if (!s_pwm[idx].active) return;
	uint32_t ch = s_pwm[idx].pwm_channel;
	pwm_channel_disable(PWM, 1u << ch);
	s_pwm[idx].active = 0;
	/* Park the pin low by giving it back to PIO and clearing it. */
	uint32_t mask = 1u << s_pwm[idx].pio_pin;
	PIOC->PIO_CODR = mask;
	PIOC->PIO_OER  = mask;
	PIOC->PIO_PER  = mask;    /* PIO controller owns the pin again */
}

/* -------------------------------------------------------------------------
 *   Vendor SETUP entrypoints
 * ------------------------------------------------------------------------- */

bool waveform_get_caps(void *out, uint16_t out_len)
{
	if (out_len < sizeof(struct Capabilities)) return false;

	struct Capabilities caps = {
		.protocol_version       = 1,
		.firmware_minor         = USB_DEVICE_MINOR_VERSION,
		.firmware_major         = USB_DEVICE_MAJOR_VERSION,
		.num_dac                = WAVE_NUM_DAC,
		/* Report only the PWM channels that actually accept BUILTIN;
		 * PWM4..7 remain in the flat channel-id space for GEN_GET_STATE
		 * but are not pinned on the Due and reject BUILTIN requests. */
		.num_pwm                = WAVE_NUM_PWM_ACTIVE,
		.num_dout               = WAVE_NUM_DOUT,
		.num_din                = WAVE_NUM_DIN,
		.num_adc                = WAVE_NUM_ADC,
		.modes_dac              = MODE_MANUAL | MODE_BUILTIN | MODE_ARBITRARY
		                        | MODE_LUT | MODE_THRESHOLD | MODE_PID,
		.modes_pwm              = MODE_MANUAL | MODE_BUILTIN,
		.modes_dout             = MODE_MANUAL | MODE_LUT | MODE_THRESHOLD | MODE_PULSE_TRIG,
		.modes_din              = 0,
		.modes_adc              = 0,
		.max_dac_sample_rate_hz = WAVE_MAX_DAC_SAMPLE_RATE_HZ,
		.max_arb_buffer_samples = WAVE_MAX_ARB_SAMPLES,
	};
	memcpy(out, &caps, sizeof(caps));
	return true;
}

/* Map a flat channel ID to (kind, index). Returns false on out-of-range. */
static bool channel_decode(uint16_t id, uint8_t *kind, uint8_t *idx)
{
	if (id < WAVE_NUM_DAC)                          { *kind = CHAN_KIND_DAC;  *idx = id;                          return true; }
	uint16_t base = WAVE_NUM_DAC;
	if (id < base + WAVE_NUM_PWM)                   { *kind = CHAN_KIND_PWM;  *idx = id - base;                   return true; }
	base += WAVE_NUM_PWM;
	if (id < base + WAVE_NUM_DOUT)                  { *kind = CHAN_KIND_DOUT; *idx = id - base;                   return true; }
	base += WAVE_NUM_DOUT;
	if (id < base + WAVE_NUM_DIN)                   { *kind = CHAN_KIND_DIN;  *idx = id - base;                   return true; }
	base += WAVE_NUM_DIN;
	if (id < base + WAVE_NUM_ADC)                   { *kind = CHAN_KIND_ADC;  *idx = id - base;                   return true; }
	return false;
}

bool waveform_get_state(uint16_t channel_id, void *out, uint16_t out_len)
{
	if (out_len < sizeof(struct ChannelState)) return false;
	uint8_t kind, idx;
	if (!channel_decode(channel_id, &kind, &idx)) return false;

	struct ChannelState st = {
		.channel_kind  = kind,
		.channel_index = idx,
		.shape         = SHAPE_OFF,
	};
	if (kind == CHAN_KIND_DAC && idx < WAVE_NUM_DAC) {
		dac_chan_t c = s_chan[idx];
		st.shape                 = c.shape;
		st.flags                 = c.flags;
		st.freq_mHz              = c.freq_mHz;
		st.duty_x10              = c.duty_x10;
		st.amplitude             = c.amplitude;
		st.offset                = c.offset;
		st.phase_offset_x16      = c.phase_offset_x16;
		st.arb_n_samples         = c.arb_n_samples;
		st.arb_loops_remaining   = c.arb_loops_remaining;
		st.arb_sample_rate_hz    = c.arb_sample_rate_hz;
		st.cur_phase_q24_8       = c.phase_q24_8;
	} else if (kind == CHAN_KIND_PWM) {
		if (idx >= WAVE_NUM_PWM_ACTIVE) {
			/* PWM 4..7 live in the channel ID space so host code
			 * can iterate 0..num_pwm symmetrically, but this
			 * hardware revision does not route them to a pin:
			 * BUILTIN rejects them and GEN_GET_STATE reports
			 * them as "reserved" via the flag bit. */
			st.flags |= CHAN_STATE_FLAG_RESERVED;
		} else if (s_pwm[idx].active) {
			st.shape    = SHAPE_SQUARE;
			st.freq_mHz = s_pwm[idx].freq_mHz;
			st.duty_x10 = s_pwm[idx].duty_x10;
		}
	} else if (kind == CHAN_KIND_DOUT) {
		/* Report whichever reactive mode owns this DOUT so the
		 * dashboard indicator reflects the firmware state. */
		if (s_thr_dout[idx].active) {
			st.shape = SHAPE_THRESHOLD;
		} else {
			for (uint8_t s = 0; s < MAX_PULSE_TRIG_SLOTS; s++) {
				if (s_pulse[s].active && s_pulse[s].out_kind == CHAN_KIND_DOUT
				                      && s_pulse[s].out_idx == idx) {
					st.shape = SHAPE_PULSE_TRIG;
					break;
				}
			}
			if (st.shape == SHAPE_OFF) {
				int8_t slot = lut_slot_for((uint16_t)(WAVE_NUM_DAC + WAVE_NUM_PWM + idx));
				if (slot >= 0 && s_lut[slot].input_src != INPUT_SRC_NONE) {
					st.shape = SHAPE_LUT;
				}
			}
		}
	}
	memcpy(out, &st, sizeof(st));
	return true;
}

bool waveform_play_builtin(uint16_t channel_id, const void *data, uint16_t len)
{
	if (len != sizeof(struct WaveBuiltinSpec)) return false;
	uint8_t kind, idx;
	if (!channel_decode(channel_id, &kind, &idx)) return false;

	const struct WaveBuiltinSpec *spec = (const struct WaveBuiltinSpec *)data;
	if (spec->shape == SHAPE_OFF || spec->shape == SHAPE_ARBITRARY) return false;
	if (spec->shape > SHAPE_SAWTOOTH && spec->shape != SHAPE_DC) return false;
	if (spec->flags != 0)            return false;
	if (spec->phase_offset_x16 != 0) return false;  /* reserved in v1 */
	if (spec->reserved1 != 0)        return false;
	if (spec->shape == SHAPE_SQUARE && spec->duty_x10 > 1000) return false;

	/* PWM channels (MODE_PWM_DUTY): only SHAPE_SQUARE is meaningful
	 * — the PWM peripheral is a digital duty-cycle generator. */
	if (kind == CHAN_KIND_PWM) {
		if (spec->shape != SHAPE_SQUARE) return false;
		if (idx >= WAVE_NUM_PWM_ACTIVE)  return false;   /* PWM4..7 not pinned on Due */
		return pwm_hw_play(idx, spec->freq_mHz, spec->duty_x10);
	}
	if (kind != CHAN_KIND_DAC) return false;        /* DOUT BUILTIN reserved for future */

	NVIC_DisableIRQ(DACC_IRQn);
	dac_chan_t *c = (dac_chan_t *)&s_chan[idx];
	bool was_active = (c->shape != SHAPE_OFF);
	c->shape            = spec->shape;
	c->flags            = spec->flags;
	c->duty_x10         = spec->duty_x10;
	c->amplitude        = spec->amplitude;
	c->offset           = spec->offset;
	c->freq_mHz         = spec->freq_mHz;
	c->phase_offset_x16 = spec->phase_offset_x16;
	if (!was_active) c->phase_q24_8 = 0;            /* fresh start; sweeps preserve via skip */
	c->arb_n_samples    = 0;
	c->arb_loops_remaining = 0;
	c->arb_sample_rate_hz  = 0;
	recompute_phase_increments_for(idx);
	NVIC_EnableIRQ(DACC_IRQn);

	update_pdc_running();
	return true;
}

bool waveform_play_arbitrary(uint16_t channel_id, const void *data, uint16_t len)
{
	if (len < sizeof(struct WaveArbHeader)) return false;
	uint8_t kind, idx;
	if (!channel_decode(channel_id, &kind, &idx)) return false;
	if (kind != CHAN_KIND_DAC) return false;

	const struct WaveArbHeader *hdr = (const struct WaveArbHeader *)data;
	if (hdr->n_samples == 0 || hdr->n_samples > WAVE_MAX_ARB_SAMPLES) return false;
	if (hdr->sample_rate_hz == 0 || hdr->sample_rate_hz > WAVE_MAX_DAC_SAMPLE_RATE_HZ) return false;

	uint16_t expected_payload = sizeof(struct WaveArbHeader)
	                          + (uint16_t)hdr->n_samples * sizeof(int16_t);
	if (len != expected_payload) return false;

	/* The DATA stage payload buffer is allocated word-aligned by the
	 * UDC SETUP code (see s_setup_buf in udi_vendor.c with
	 * COMPILER_WORD_ALIGNED), and our header is exactly 8 bytes, so
	 * the int16 sample array starts at a 16-bit aligned offset.
	 * Cast through void* to silence the cast-align warning since we
	 * know the alignment is guaranteed at the source. */
	const void *samples_v = (const void *)((const uint8_t *)data + sizeof(struct WaveArbHeader));
	const int16_t *samples = (const int16_t *)samples_v;

	NVIC_DisableIRQ(DACC_IRQn);
	dac_chan_t *c = (dac_chan_t *)&s_chan[idx];
	memcpy((void *)s_arb_buf[idx], samples, (size_t)hdr->n_samples * sizeof(int16_t));
	c->shape                = SHAPE_ARBITRARY;
	c->flags                = 0;
	c->amplitude            = 4095;     /* default 1× scale */
	c->offset               = 2048;     /* default mid-rail */
	c->arb_n_samples        = hdr->n_samples;
	c->arb_loops_remaining  = hdr->loop_count;     /* 0 = infinite */
	c->arb_sample_rate_hz   = hdr->sample_rate_hz;
	c->arb_pos_q24_8        = 0;
	c->phase_q24_8          = 0;
	recompute_phase_increments_for(idx);
	NVIC_EnableIRQ(DACC_IRQn);

	update_pdc_running();
	return true;
}

bool waveform_stop(uint16_t channel_id)
{
	uint8_t kind, idx;
	if (!channel_decode(channel_id, &kind, &idx)) return false;
	if (kind == CHAN_KIND_DAC) {
		NVIC_DisableIRQ(DACC_IRQn);
		s_chan[idx].shape = SHAPE_OFF;
		s_thr_dac[idx].active = 0;
		s_pid[idx].active     = 0;
		NVIC_EnableIRQ(DACC_IRQn);
		update_pdc_running();
	} else if (kind == CHAN_KIND_PWM) {
		pwm_hw_stop(idx);
	} else if (kind == CHAN_KIND_DOUT) {
		s_thr_dout[idx].active = 0;
	}
	/* Clear any LUT bound to this channel (DAC0/DAC1/DOUT slots). */
	int8_t lut = lut_slot_for(channel_id);
	if (lut >= 0) {
		s_lut[lut].input_src = INPUT_SRC_NONE;
	}
	/* Clear any pulse trig that targets this channel. */
	for (uint8_t i = 0; i < MAX_PULSE_TRIG_SLOTS; i++) {
		if (s_pulse[i].active && s_pulse[i].out_kind == kind && s_pulse[i].out_idx == idx) {
			s_pulse[i].active = 0;
			s_pulse[i].in_pulse = 0;
		}
	}
	return true;
}

/* -------- Reactive: SHAPE_LUT -------- */
bool waveform_play_lut(uint16_t channel_id, const void *data, uint16_t len)
{
	if (len < sizeof(struct WaveLutSpec)) return false;
	uint8_t kind, idx;
	if (!channel_decode(channel_id, &kind, &idx)) return false;
	if (kind != CHAN_KIND_DAC && kind != CHAN_KIND_DOUT) return false;

	const struct WaveLutSpec *spec = (const struct WaveLutSpec *)data;
	if (spec->reserved0 != 0 || spec->reserved1 != 0) return false;
	if (spec->n_entries == 0 || spec->n_entries > 4096) return false;
	if (spec->input_src != INPUT_SRC_ADC && spec->input_src != INPUT_SRC_DIN_MASK) return false;
	uint16_t expected = sizeof(struct WaveLutSpec) + (uint16_t)spec->n_entries * sizeof(int16_t);
	if (len != expected) return false;

	int8_t slot = lut_slot_for(channel_id);
	if (slot < 0) return false;

	const void *entries_v = (const void *)((const uint8_t *)data + sizeof(struct WaveLutSpec));
	const int16_t *entries = (const int16_t *)entries_v;

	NVIC_DisableIRQ(DACC_IRQn);
	memcpy((void *)s_lut_data[slot], entries, (size_t)spec->n_entries * sizeof(int16_t));
	s_lut[slot].entries     = s_lut_data[slot];
	s_lut[slot].input_src   = spec->input_src;
	s_lut[slot].input_arg   = spec->input_arg;
	s_lut[slot].n_entries   = spec->n_entries;
	s_lut[slot].output_mask = spec->output_mask;
	s_lut[slot].out_kind    = kind;
	s_lut[slot].out_idx     = idx;
	if (kind == CHAN_KIND_DAC) {
		s_chan[idx].shape = SHAPE_LUT;     /* freeze open-loop refill */
	}
	NVIC_EnableIRQ(DACC_IRQn);
	update_pdc_running();
	/* Apply once immediately so output isn't stale until the next ISR. */
	lut_apply((uint8_t)slot);
	return true;
}

/* -------- Reactive: SHAPE_THRESHOLD -------- */
bool waveform_play_threshold(uint16_t channel_id, const void *data, uint16_t len)
{
	if (len != sizeof(struct WaveThresholdSpec)) return false;
	uint8_t kind, idx;
	if (!channel_decode(channel_id, &kind, &idx)) return false;
	if (kind != CHAN_KIND_DAC && kind != CHAN_KIND_DOUT) return false;
	const struct WaveThresholdSpec *spec = (const struct WaveThresholdSpec *)data;
	if (spec->reserved0 != 0 || spec->reserved1 != 0) return false;
	if (spec->thr_low > spec->thr_high) return false;

	if (kind == CHAN_KIND_DAC) {
		NVIC_DisableIRQ(DACC_IRQn);
		s_thr_dac[idx].spec = *spec;
		s_thr_dac[idx].cur_high = 0;
		s_thr_dac[idx].out_kind = CHAN_KIND_DAC;
		s_thr_dac[idx].out_idx  = idx;
		s_thr_dac[idx].active   = 1;
		s_chan[idx].shape       = SHAPE_THRESHOLD;
		NVIC_EnableIRQ(DACC_IRQn);
		update_pdc_running();
		threshold_eval(CHAN_KIND_DAC, idx, &s_thr_dac[idx]);
	} else {
		s_thr_dout[idx].spec = *spec;
		s_thr_dout[idx].cur_high = 0;
		s_thr_dout[idx].out_kind = CHAN_KIND_DOUT;
		s_thr_dout[idx].out_idx  = idx;
		s_thr_dout[idx].active   = 1;
		threshold_eval(CHAN_KIND_DOUT, idx, &s_thr_dout[idx]);
	}
	return true;
}

/* -------- Reactive: SHAPE_PULSE_TRIG -------- */
bool waveform_play_pulse_trig(uint16_t channel_id, const void *data, uint16_t len)
{
	if (len != sizeof(struct WavePulseSpec)) return false;
	uint8_t kind, idx;
	if (!channel_decode(channel_id, &kind, &idx)) return false;
	if (kind != CHAN_KIND_DOUT) return false;     /* v1: DOUT only */
	const struct WavePulseSpec *spec = (const struct WavePulseSpec *)data;
	if (spec->reserved0 != 0) return false;
	if (spec->edge < 1 || spec->edge > 3) return false;
	if (spec->input_din_bit >= WAVE_NUM_DIN) return false;
	if (spec->duration_us == 0 || spec->duration_us > 1000000u) return false;
	if (spec->cooldown_us > 1000000u) return false;
	if (spec->active_level > 1) return false;

	/* Find a free pulse slot, or reuse the slot already bound to
	 * this output (re-arm semantics). */
	int8_t slot = -1;
	for (uint8_t i = 0; i < MAX_PULSE_TRIG_SLOTS; i++) {
		if (s_pulse[i].active && s_pulse[i].out_kind == kind && s_pulse[i].out_idx == idx) { slot = (int8_t)i; break; }
	}
	if (slot < 0) {
		for (uint8_t i = 0; i < MAX_PULSE_TRIG_SLOTS; i++) {
			if (!s_pulse[i].active) { slot = (int8_t)i; break; }
		}
	}
	if (slot < 0) return false;     /* all slots full */

	s_pulse[slot].spec = *spec;
	s_pulse[slot].out_kind = kind;
	s_pulse[slot].out_idx = idx;
	s_pulse[slot].in_pulse = 0;
	s_pulse[slot].cooldown_remaining_us = 0;
	s_pulse[slot].active = 1;
	return true;
}

/* -------- Reactive: SHAPE_PID (v3) -------- */
bool waveform_play_pid(uint16_t channel_id, const void *data, uint16_t len)
{
	if (len != sizeof(struct WavePidSpec)) return false;
	uint8_t kind, idx;
	if (!channel_decode(channel_id, &kind, &idx)) return false;
	if (kind != CHAN_KIND_DAC) return false;     /* PID outputs to DAC only in v3 */
	const struct WavePidSpec *spec = (const struct WavePidSpec *)data;
	if (spec->reserved0 != 0) return false;
	if (spec->input_src != INPUT_SRC_ADC) return false;     /* v3 supports ADC source only */
	if ((spec->input_arg & 0x07) >= WAVE_NUM_ADC) return false;
	if (spec->out_min > spec->out_max) return false;
	if (spec->out_max > DACC_VAL_MASK) return false;
	if (spec->integral_clamp < 0) return false;

	NVIC_DisableIRQ(DACC_IRQn);
	/* Whether or not we were already in PID mode, accept new spec
	 * seamlessly: keep integrator unless this is a fresh start. */
	bool was_active = s_pid[idx].active;
	s_pid[idx].spec   = *spec;
	s_pid[idx].active = 1;
	s_pid[idx].out_idx = idx;
	if (!was_active) {
		s_pid[idx].integral_q16_16  = 0;
		s_pid[idx].prev_error_q16_16 = 0;
		s_pid[idx].prev_d_q16_16    = 0;
	}
	s_chan[idx].shape = SHAPE_PID;
	NVIC_EnableIRQ(DACC_IRQn);
	update_pdc_running();
	return true;
}

bool waveform_set_dac_clock(uint32_t clock_hz)
{
	if (clock_hz == 0 || clock_hz > WAVE_MAX_DAC_SAMPLE_RATE_HZ) return false;
	NVIC_DisableIRQ(DACC_IRQn);
	s_dac_clock_hz = clock_hz;
	recompute_phase_increments();
	NVIC_EnableIRQ(DACC_IRQn);
	if (s_pdc_running) {
		/* Reprogram the TC for the new rate. There may be a 1-sample
		 * blip on the analog output; documented in PROTOCOL.md §4.2. */
		tc_dac_stop();
		tc_dac_setup(tc_dac_trigger_hz());
	}
	return true;
}

uint32_t waveform_get_dac_clock(void) { return s_dac_clock_hz; }

bool waveform_set_adc_rate(uint32_t rate_hz)
{
	if (rate_hz == 0 || rate_hz > WAVE_MAX_DAC_SAMPLE_RATE_HZ) return false;

	/* Switch ADC from free-running mode to TC0 channel 1 trigger so
	 * the per-channel sampling rate becomes a function of `rate_hz`.
	 * Each TC trigger fires one sweep through all 8 enabled ADC
	 * channels (PDC RX continues to fill g_adc_buf as before). */
	pmc_enable_periph_clk(ID_TC0);   /* idempotent */

	uint32_t tc_clock = sysclk_get_main_hz() / 2u;     /* TIMER_CLOCK1 = MCK/2 */
	uint32_t rc = tc_clock / rate_hz;
	if (rc < 4u)        rc = 4u;
	if (rc > 0xFFFFu)   rc = 0xFFFFu;

	tc_init(TC0, 1,
	        TC_CMR_TCCLKS_TIMER_CLOCK1 |
	        TC_CMR_WAVE                 |
	        TC_CMR_WAVSEL_UP_RC         |
	        TC_CMR_ACPA_SET             |
	        TC_CMR_ACPC_CLEAR);
	tc_write_ra(TC0, 1, rc / 2u);
	tc_write_rc(TC0, 1, rc);
	tc_start(TC0, 1);

	/* Reconfigure ADC_MR: clear FREE_RUN bit, set TRGEN, route
	 * TRGSEL=ADC_TRIG2 → TIOA from TC0 channel 1. */
	uint32_t mr = ADC->ADC_MR;
	mr &= ~(ADC_MR_FREERUN | ADC_MR_TRGEN | ADC_MR_TRGSEL_Msk);
	mr |=  ADC_MR_TRGEN_EN | ADC_MR_TRGSEL_ADC_TRIG2;
	ADC->ADC_MR = mr;

	s_adc_rate_hz = rate_hz;
	return true;
}

uint32_t waveform_get_adc_rate(void) { return s_adc_rate_hz; }

