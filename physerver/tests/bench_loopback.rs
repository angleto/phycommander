//! HTTP-driven bench loopback test suite.
//!
//! Turns the benched Arduino Due into a self-diagnosing rig by talking to a
//! live physerver over HTTP (http://127.0.0.1:8080 by default). Each test
//! drives a DAC/PWM/GPIO endpoint and reads the ADC or DIN it is wired to,
//! turning "I guess DAC0 is broken, let me probe 10 things" into a 30-second
//! regression check.
//!
//! Gates:
//!   - `#[ignore]` on every test: `cargo test` won't touch the hardware.
//!   - `PHYCMD_BENCH=1` env required; otherwise the test body exits early
//!     with a visible message. This protects against
//!     `cargo test -- --ignored` on a dev laptop accidentally talking to
//!     a hypothetical running physerver.
//!
//! Run on the bench host:
//!   PHYCMD_BENCH=1 cargo test --test bench_loopback --release -- \
//!       --ignored --test-threads=1 --nocapture
//!
//! Default wiring (override via env vars, all take a decimal ADC-slot index
//! 0..7 matching the firmware adc[] array, i.e. Due silkscreen A1..A6,A10,A11):
//!
//!   Signal   Due pin   ADC slot   Env var override
//!   DAC0     A1        0          PHYCMD_DAC0_ADC  (silicon fault: expect low swing)
//!   DAC1     A2        1          PHYCMD_DAC1_ADC
//!   pwm0     A4 (D9)   3          PHYCMD_PWM0_ADC
//!   pwm1     A3 (D8)   2          PHYCMD_PWM1_ADC
//!   pwm2     A11 (D7)  7          PHYCMD_PWM2_ADC
//!   pwm3     A10 (D6)  6          PHYCMD_PWM3_ADC
//!   pwm4     A5 (D10)  4          PHYCMD_PWM4_ADC
//!   pwm5     A6 (D11)  5          PHYCMD_PWM5_ADC
//!
//! Other env vars:
//!   PHYCMD_URL       base URL, default http://127.0.0.1:8080
//!   PHYCMD_SKIP_DAC0 set to 1 to skip the "DAC0 silicon fault" check
//!   PHYCMD_SKIP_PWM  comma-list of pwm indices to skip (e.g. "2,3")

use std::time::Duration;

use serde::Deserialize;

const DAC_MAX: u16 = 4095;

// HTTP path adds latency + ADC averaging window vs selftest's direct-serial
// path, so widen the tolerance. 150 LSB out of 4096 ≈ 3.7% — still well
// under the "something is fundamentally wrong" threshold but forgiving
// enough that a couple of LSB of noise don't flap the build.
const ADC_LINEARITY_TOLERANCE: f64 = 150.0;
const ADC_HIGH_THRESHOLD: u16 = 3600;
const ADC_LOW_THRESHOLD: u16 = 400;

// Idle noise cap. The FREE-RUN ADC on this board sits around 2-6 LSB stddev;
// 30 gives headroom for SMPS/60Hz pickup without hiding a genuine stuck-
// channel regression.
const ADC_IDLE_STDDEV_MAX: f64 = 30.0;

// ===========================================================================
//  HTTP client
// ===========================================================================

struct BenchHttpClient {
    base_url: String,
    agent: ureq::Agent,
}

impl BenchHttpClient {
    fn new() -> Self {
        let base_url = std::env::var("PHYCMD_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(2))
            .timeout(Duration::from_secs(3))
            .build();
        Self { base_url, agent }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn set_dac(&self, channel: u8, value: u16) {
        let url = self.url("/api/dac/set");
        self.agent
            .post(&url)
            .send_json(serde_json::json!({ "channel": channel, "value": value }))
            .unwrap_or_else(|e| panic!("set_dac({}, {}): {}", channel, value, e));
    }

    fn set_gpio(&self, pin: u8, value: bool) {
        let url = self.url("/api/gpio/set");
        self.agent
            .post(&url)
            .send_json(serde_json::json!({ "pin": pin, "value": value }))
            .unwrap_or_else(|e| panic!("set_gpio({}, {}): {}", pin, value, e));
    }

    /// Drive a PWM channel via the firmware fn-gen control plane. We use
    /// `square` @ 1 kHz and vary `duty` to get a proportional average on the
    /// ADC (no RC filter needed: the ADC samples at 8 kHz, so a 1 kHz square
    /// averages to duty×VCC across ~4 ADC samples per period).
    fn play_pwm_duty(&self, channel: u8, duty: f32) {
        let url = self.url(&format!("/api/fngen/play_builtin/pwm{}", channel));
        let body = serde_json::json!({
            "shape": "square",
            "freq_hz": 1000.0,
            "amplitude": 4095,
            "offset": 0,
            "duty": duty,
        });
        self.agent
            .post(&url)
            .send_json(body)
            .unwrap_or_else(|e| panic!("play_pwm_duty(pwm{}, {}): {}", channel, duty, e));
    }

    fn stop_pwm(&self, channel: u8) {
        let url = self.url(&format!("/api/fngen/stop/pwm{}", channel));
        // Empty-body POST. Using send_string("") so ureq sets Content-Length: 0.
        let _ = self.agent.post(&url).send_string("");
    }

    fn read_adc(&self) -> [u16; 8] {
        let url = self.url("/api/adc/read");
        let resp: AdcResponse = self
            .agent
            .get(&url)
            .call()
            .and_then(|r| r.into_json().map_err(ureq::Error::from))
            .unwrap_or_else(|e| panic!("read_adc: {}", e));
        resp.channels
    }

    fn read_digital_in(&self) -> u16 {
        let url = self.url("/api/status");
        let resp: StatusMini = self
            .agent
            .get(&url)
            .call()
            .and_then(|r| r.into_json().map_err(ureq::Error::from))
            .unwrap_or_else(|e| panic!("read_status: {}", e));
        resp.digital_in
    }

    fn read_adc_avg(&self, samples: usize) -> [f64; 8] {
        let mut sums = [0.0f64; 8];
        for _ in 0..samples {
            let ch = self.read_adc();
            for i in 0..8 {
                sums[i] += ch[i] as f64;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut out = [0.0f64; 8];
        for i in 0..8 {
            out[i] = sums[i] / samples as f64;
        }
        out
    }

    /// Reset every host-side output so later tests don't inherit leaked state.
    fn reset_all_outputs(&self) {
        self.set_dac(0, 0);
        self.set_dac(1, 0);
        for p in 0..8u8 {
            self.stop_pwm(p);
        }
        for g in 0..16u8 {
            self.set_gpio(g, false);
        }
    }
}

#[derive(Deserialize)]
struct AdcResponse {
    channels: [u16; 8],
}

#[derive(Deserialize)]
struct StatusMini {
    digital_in: u16,
}

// ===========================================================================
//  Helpers
// ===========================================================================

fn env_adc_slot(var: &str, default: usize) -> usize {
    std::env::var(var).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn bench_enabled() -> bool {
    std::env::var("PHYCMD_BENCH").ok().as_deref() == Some("1")
}

/// Open a client, or log & return None if PHYCMD_BENCH isn't set. Using this
/// early-return pattern rather than a macro so the diagnostic shows the
/// actual test name (via --nocapture) instead of a generic "skipped".
fn connect_or_skip(test_name: &str) -> Option<BenchHttpClient> {
    if !bench_enabled() {
        eprintln!(
            "[bench_loopback::{}] PHYCMD_BENCH=1 not set — skipping (no hardware)",
            test_name
        );
        return None;
    }
    let client = BenchHttpClient::new();
    eprintln!("[bench_loopback::{}] target: {}", test_name, client.base_url);
    Some(client)
}

fn linear_regression(samples: &[(u16, f64)]) -> (f64, f64) {
    let n = samples.len() as f64;
    let xs: Vec<f64> = samples.iter().map(|(x, _)| *x as f64).collect();
    let ys: Vec<f64> = samples.iter().map(|(_, y)| *y).collect();
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let num: f64 = xs.iter().zip(ys.iter()).map(|(x, y)| (x - mean_x) * (y - mean_y)).sum();
    let den: f64 = xs.iter().map(|x| (x - mean_x).powi(2)).sum();
    let slope = if den > 0.0 { num / den } else { 0.0 };
    let intercept = mean_y - slope * mean_x;
    let ss_tot: f64 = ys.iter().map(|y| (y - mean_y).powi(2)).sum();
    let ss_res: f64 = xs
        .iter()
        .zip(ys.iter())
        .map(|(x, y)| {
            let pred = slope * x + intercept;
            (y - pred).powi(2)
        })
        .sum();
    let r2 = if ss_tot > 0.0 { 1.0 - ss_res / ss_tot } else { 0.0 };
    (slope, r2)
}

// ===========================================================================
//  Test 1: DAC linearity (DAC1)
// ===========================================================================

#[test]
#[ignore]
fn bench_dac1_linearity() {
    let Some(client) = connect_or_skip("bench_dac1_linearity") else {
        return;
    };
    let adc_slot = env_adc_slot("PHYCMD_DAC1_ADC", 1);

    client.set_dac(0, 0);
    client.set_dac(1, 0);
    std::thread::sleep(Duration::from_millis(30));

    let steps = 16;
    let step_size = DAC_MAX / steps;
    let mut samples: Vec<(u16, f64)> = Vec::new();

    eprintln!("[dac1] linearity sweep -> ADC slot {}:", adc_slot);
    for i in 0..=steps {
        let v = (i * step_size).min(DAC_MAX);
        client.set_dac(1, v);
        std::thread::sleep(Duration::from_millis(25));
        let avg = client.read_adc_avg(5);
        eprintln!("  DAC1={:4}  ADC[{}]={:7.1}", v, adc_slot, avg[adc_slot]);
        samples.push((v, avg[adc_slot]));
    }
    client.set_dac(1, 0);

    let (slope, r2) = linear_regression(&samples);
    let (first, last) = (samples.first().unwrap().1, samples.last().unwrap().1);
    let swing = last - first;
    eprintln!(
        "[dac1] swing={:.0} LSB, slope={:.3}, R²={:.4}",
        swing, slope, r2
    );

    assert!(
        swing > 2000.0,
        "DAC1 swing only {:.0} LSB — expected >2000 (is DAC1 wired to ADC[{}]?)",
        swing, adc_slot
    );
    assert!(
        (0.55..=0.85).contains(&slope),
        "DAC1 slope {:.3} out of [0.55, 0.85] — DAC=3.3V ref vs ADC=3.3V ref but output stage caps at ~2.7V on SAM3X, so ~0.7 is expected",
        slope
    );
    assert!(r2 > 0.98, "DAC1 linearity R² too low: {:.4}", r2);
}

// ===========================================================================
//  Test 2: DAC0 silicon-fault regression check
//
//  DAC0 on this particular chip has a damaged output stage: ~40 mV swing out
//  of a 2200 mV nominal (see memory/bench_chip_silicon_faults.md). We flag
//  the fault by asserting the swing stays LOW. If someone swaps in a fresh
//  SAM3X, this test will fail with a diagnostic asking to re-enable DAC0 in
//  the dashboard and flip this assertion.
// ===========================================================================

#[test]
#[ignore]
fn bench_dac0_silicon_fault_present() {
    if std::env::var("PHYCMD_SKIP_DAC0").ok().as_deref() == Some("1") {
        eprintln!("[dac0] PHYCMD_SKIP_DAC0=1 — skipping");
        return;
    }
    let Some(client) = connect_or_skip("bench_dac0_silicon_fault_present") else {
        return;
    };
    let adc_slot = env_adc_slot("PHYCMD_DAC0_ADC", 0);

    client.set_dac(0, 0);
    client.set_dac(1, 0);
    std::thread::sleep(Duration::from_millis(30));
    let avg_lo = client.read_adc_avg(10)[adc_slot];

    client.set_dac(0, DAC_MAX);
    std::thread::sleep(Duration::from_millis(30));
    let avg_hi = client.read_adc_avg(10)[adc_slot];

    client.set_dac(0, 0);

    let swing = avg_hi - avg_lo;
    eprintln!(
        "[dac0] lo={:.0}, hi={:.0}, swing={:.0} LSB (expected << 2000 due to silicon fault)",
        avg_lo, avg_hi, swing
    );

    assert!(
        swing < 400.0,
        "DAC0 swing={:.0} LSB — the silicon fault seems to be gone! \
         Chip replaced? Re-enable DAC0 in the dashboard and update this test.",
        swing
    );
}

// ===========================================================================
//  Test 3: PWM duty endpoints for each of the 6 firmware PWM channels
// ===========================================================================

#[test]
#[ignore]
fn bench_pwm_duty_endpoints() {
    let Some(client) = connect_or_skip("bench_pwm_duty_endpoints") else {
        return;
    };

    let skip: Vec<u8> = std::env::var("PHYCMD_SKIP_PWM")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    // Defaults from current bench wiring. Overridable via env. Slot 99 is a
    // sentinel meaning "not wired — skip".
    let wiring: [(u8, usize); 6] = [
        (0, env_adc_slot("PHYCMD_PWM0_ADC", 3)),
        (1, env_adc_slot("PHYCMD_PWM1_ADC", 2)),
        (2, env_adc_slot("PHYCMD_PWM2_ADC", 7)),
        (3, env_adc_slot("PHYCMD_PWM3_ADC", 6)),
        (4, env_adc_slot("PHYCMD_PWM4_ADC", 4)),
        (5, env_adc_slot("PHYCMD_PWM5_ADC", 5)),
    ];

    client.reset_all_outputs();
    std::thread::sleep(Duration::from_millis(30));

    let mut failures: Vec<String> = Vec::new();
    for (pwm, slot) in wiring {
        if skip.contains(&pwm) || slot >= 8 {
            eprintln!("[pwm{}] -> ADC[{}]: skipped", pwm, slot);
            continue;
        }

        client.play_pwm_duty(pwm, 0.0);
        std::thread::sleep(Duration::from_millis(60));
        let low = client.read_adc_avg(8)[slot];

        client.play_pwm_duty(pwm, 1.0);
        std::thread::sleep(Duration::from_millis(60));
        let high = client.read_adc_avg(8)[slot];

        client.stop_pwm(pwm);

        eprintln!(
            "[pwm{}] -> ADC[{}]: duty=0 -> {:6.0}, duty=1 -> {:6.0}, swing={:6.0}",
            pwm, slot, low, high, high - low
        );

        if low >= ADC_LOW_THRESHOLD as f64 {
            failures.push(format!(
                "pwm{} duty=0: ADC[{}]={:.0} (threshold <{})",
                pwm, slot, low, ADC_LOW_THRESHOLD
            ));
        }
        if high <= ADC_HIGH_THRESHOLD as f64 {
            failures.push(format!(
                "pwm{} duty=1: ADC[{}]={:.0} (threshold >{})",
                pwm, slot, high, ADC_HIGH_THRESHOLD
            ));
        }
    }

    client.reset_all_outputs();
    assert!(failures.is_empty(), "PWM endpoint failures:\n  {}", failures.join("\n  "));
}

// ===========================================================================
//  Test 4: PWM duty monotonic response (5 steps)
//
//  At 1 kHz carrier vs 8 kHz ADC sample rate, the ADC averages ~4 samples per
//  PWM period — good enough for monotonicity but not for strict linearity
//  (aliasing beat between carrier and sampler). Tolerance picks up the slack.
// ===========================================================================

#[test]
#[ignore]
fn bench_pwm_duty_monotonic() {
    let Some(client) = connect_or_skip("bench_pwm_duty_monotonic") else {
        return;
    };

    let pwm: u8 = std::env::var("PHYCMD_MONOTONIC_PWM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let slot = env_adc_slot(
        &format!("PHYCMD_PWM{}_ADC", pwm),
        match pwm {
            0 => 3,
            1 => 2,
            2 => 7,
            3 => 6,
            4 => 4,
            5 => 5,
            _ => 99,
        },
    );
    if slot >= 8 {
        eprintln!("[pwm{} monotonic] not wired — skip", pwm);
        return;
    }

    client.reset_all_outputs();
    std::thread::sleep(Duration::from_millis(30));

    let duties = [0.0f32, 0.25, 0.50, 0.75, 1.0];
    let mut readings = Vec::with_capacity(duties.len());
    for &d in &duties {
        client.play_pwm_duty(pwm, d);
        std::thread::sleep(Duration::from_millis(60));
        let v = client.read_adc_avg(10)[slot];
        eprintln!("[pwm{} monotonic] duty={:.2} -> ADC[{}]={:7.1}", pwm, d, slot, v);
        readings.push(v);
    }
    client.stop_pwm(pwm);

    // Monotonicity: each step should be >= previous step minus tolerance.
    let tol = ADC_LINEARITY_TOLERANCE;
    for i in 1..readings.len() {
        assert!(
            readings[i] + tol >= readings[i - 1],
            "pwm{} not monotonic at step {}: {:.0} after {:.0} (tol {:.0})",
            pwm, i, readings[i], readings[i - 1], tol
        );
    }
    // And overall swing should cover most of the range.
    let swing = readings.last().unwrap() - readings.first().unwrap();
    assert!(
        swing > 2500.0,
        "pwm{} monotonic swing only {:.0} — ADC aliasing?",
        pwm, swing
    );
}

// ===========================================================================
//  Test 5: GPIO walking ones
// ===========================================================================

#[test]
#[ignore]
fn bench_gpio_walking_ones() {
    let Some(client) = connect_or_skip("bench_gpio_walking_ones") else {
        return;
    };

    // Clear all first
    for i in 0..16u8 {
        client.set_gpio(i, false);
    }
    std::thread::sleep(Duration::from_millis(30));

    let mut failures: Vec<String> = Vec::new();
    for bit in 0..16u8 {
        client.set_gpio(bit, true);
        std::thread::sleep(Duration::from_millis(20));
        let din = client.read_digital_in();
        let expected = 1u16 << bit;
        if din != expected {
            failures.push(format!(
                "walking-1 bit {}: DOUT=0x{:04X}, DIN=0x{:04X}",
                bit, expected, din
            ));
        }
        client.set_gpio(bit, false);
    }

    assert!(failures.is_empty(), "GPIO walking-1 failures:\n  {}", failures.join("\n  "));
}

// ===========================================================================
//  Test 6: GPIO walking zeros
// ===========================================================================

#[test]
#[ignore]
fn bench_gpio_walking_zeros() {
    let Some(client) = connect_or_skip("bench_gpio_walking_zeros") else {
        return;
    };

    for i in 0..16u8 {
        client.set_gpio(i, true);
    }
    std::thread::sleep(Duration::from_millis(30));

    let mut failures: Vec<String> = Vec::new();
    for bit in 0..16u8 {
        client.set_gpio(bit, false);
        std::thread::sleep(Duration::from_millis(20));
        let din = client.read_digital_in();
        let expected = 0xFFFFu16 & !(1u16 << bit);
        if din != expected {
            failures.push(format!(
                "walking-0 bit {}: DOUT=0x{:04X}, DIN=0x{:04X}",
                bit, expected, din
            ));
        }
        client.set_gpio(bit, true);
    }

    for i in 0..16u8 {
        client.set_gpio(i, false);
    }
    assert!(failures.is_empty(), "GPIO walking-0 failures:\n  {}", failures.join("\n  "));
}

// ===========================================================================
//  Test 7: ADC idle stability
//
//  With every output off, each ADC slot should settle at some voltage
//  (dictated by whatever's wired to it) with low noise. The important
//  assertion is *not* that any particular value shows up — it's that no
//  slot reads a dead 2048 (0x800) with zero variance, which is the
//  signature of the pre-FREE-RUN PDC bug we fixed in ffde516.
// ===========================================================================

#[test]
#[ignore]
fn bench_adc_idle_stability() {
    let Some(client) = connect_or_skip("bench_adc_idle_stability") else {
        return;
    };

    client.reset_all_outputs();
    std::thread::sleep(Duration::from_millis(100));

    let n = 100;
    let mut samples: Vec<[u16; 8]> = Vec::with_capacity(n);
    for _ in 0..n {
        samples.push(client.read_adc());
        std::thread::sleep(Duration::from_millis(4));
    }

    let mut failures: Vec<String> = Vec::new();
    for ch in 0..8 {
        let vals: Vec<f64> = samples.iter().map(|s| s[ch] as f64).collect();
        let mean = vals.iter().sum::<f64>() / vals.len() as f64;
        let var = vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / vals.len() as f64;
        let std = var.sqrt();
        let min = vals.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        eprintln!(
            "[idle] ADC[{}]: mean={:7.1}, std={:5.2}, range=[{:4.0},{:4.0}]",
            ch, mean, std, min, max
        );

        // Stuck-0x800 regression: mean ≈ 2048 with near-zero variance.
        if std < 0.2 && (mean - 2048.0).abs() < 5.0 {
            failures.push(format!(
                "ADC[{}] stuck at 0x800 (mean={:.1}, std={:.2}) — FREE-RUN regression",
                ch, mean, std
            ));
        }
        // Generic excess-noise check.
        if std > ADC_IDLE_STDDEV_MAX {
            failures.push(format!(
                "ADC[{}] noisy: std={:.1} > {}",
                ch, std, ADC_IDLE_STDDEV_MAX
            ));
        }
    }

    assert!(failures.is_empty(), "ADC idle stability:\n  {}", failures.join("\n  "));
}
