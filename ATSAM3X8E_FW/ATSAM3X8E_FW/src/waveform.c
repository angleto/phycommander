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

#define PINGPONG_SAMPLES   1024u   /* per ping-pong half (interleaved both channels) */
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

	NVIC_SetPriority(DACC_IRQn, 1);   /* below UOTGHS=0, above all else */
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
		uint32_t tag = (ch_idx == 0) ? DACC_TAG_CH0 : DACC_TAG_CH1;
		return ((uint32_t)v & DACC_VAL_MASK) | tag;
	}
	default: unit = 0; break;
	}

	/* Common scale path for built-in shapes. unit is in
	 * [-32767, +32767]; we want amplitude/2 swing centred on offset. */
	int32_t scaled = ((int32_t)c->amplitude * unit) >> 16;     /* halved by /2 of amplitude */
	int32_t v = (int32_t)c->offset + scaled;
	if (v < 0) v = 0;
	if (v > (int32_t)DACC_VAL_MASK) v = DACC_VAL_MASK;

	uint32_t tag = (ch_idx == 0) ? DACC_TAG_CH0 : DACC_TAG_CH1;
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
			/* Channel is in MANUAL — keep emitting the last DAC value
			 * (which is whatever main.c::apply_command_frame wrote).
			 * We use offset as the stand-in: the host sets it via
			 * GEN_STOP fallback (or it was 0 at boot). For pure
			 * idleness this just outputs 0 which is benign. */
			buf[i] = ((uint32_t)c0.offset & DACC_VAL_MASK) | DACC_TAG_CH0;
		}

		if (c1.shape != SHAPE_OFF) {
			c1.phase_q24_8 = ph1;
			buf[i + 1] = channel_sample_word(1, &c1);
			ph1 += inc1;
		} else {
			buf[i + 1] = ((uint32_t)c1.offset & DACC_VAL_MASK) | DACC_TAG_CH1;
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
	}
	s_dac_clock_hz = WAVE_DEFAULT_DAC_CLOCK_HZ;
	s_adc_rate_hz  = WAVE_DEFAULT_DAC_CLOCK_HZ;
	s_pdc_running  = false;

	sin_lut_init();
}

void waveform_stop_all(void)
{
	NVIC_DisableIRQ(DACC_IRQn);
	for (uint8_t i = 0; i < WAVE_NUM_DAC; i++) {
		s_chan[i].shape = SHAPE_OFF;
	}
	update_pdc_running();
}

bool waveform_dac_is_generating(uint8_t dac_idx)
{
	if (dac_idx >= WAVE_NUM_DAC) return false;
	return s_chan[dac_idx].shape != SHAPE_OFF;
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
		.num_pwm                = WAVE_NUM_PWM,
		.num_dout               = WAVE_NUM_DOUT,
		.num_din                = WAVE_NUM_DIN,
		.num_adc                = WAVE_NUM_ADC,
		.modes_dac              = MODE_MANUAL | MODE_BUILTIN | MODE_ARBITRARY,
		.modes_pwm              = MODE_MANUAL,
		.modes_dout             = MODE_MANUAL,
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
	}
	memcpy(out, &st, sizeof(st));
	return true;
}

bool waveform_play_builtin(uint16_t channel_id, const void *data, uint16_t len)
{
	if (len != sizeof(struct WaveBuiltinSpec)) return false;
	uint8_t kind, idx;
	if (!channel_decode(channel_id, &kind, &idx)) return false;
	if (kind != CHAN_KIND_DAC) return false;        /* PWM/DOUT BUILTIN reserved for future */

	const struct WaveBuiltinSpec *spec = (const struct WaveBuiltinSpec *)data;
	if (spec->shape == SHAPE_OFF || spec->shape == SHAPE_ARBITRARY) return false;
	if (spec->shape > SHAPE_SAWTOOTH && spec->shape != SHAPE_DC) return false;
	if (spec->flags != 0)            return false;
	if (spec->phase_offset_x16 != 0) return false;  /* reserved in v1 */
	if (spec->reserved1 != 0)        return false;
	if (spec->shape == SHAPE_SQUARE && spec->duty_x10 > 1000) return false;

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
		NVIC_EnableIRQ(DACC_IRQn);
		update_pdc_running();
	}
	/* Other kinds: stop is a no-op (they have no GENERATOR mode in v1). */
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
	/* ADC TC reconfig is a follow-up step (task #52). For now we
	 * just store the value so GET_RATE roundtrips cleanly. The
	 * existing free-running ADC keeps working at its native rate. */
	s_adc_rate_hz = rate_hz;
	return true;
}

uint32_t waveform_get_adc_rate(void) { return s_adc_rate_hz; }

