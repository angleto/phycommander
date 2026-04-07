/// Hardware Self-Test Suite per PhyCommander
///
/// Test di auto-diagnostica con connessioni loopback fisiche:
///   - DAC0 -> ADC ch0, DAC1 -> ADC ch1  (analogico)
///   - DOUT[0..15] -> DIN[0..15]          (digitale)
///   - Toggle GPIO per misura frequenza   (onda quadra)
///
/// Il firmware usa un protocollo raw senza header/CRC:
///   Comando: [0-1]=header(ignorato), [2-3]=digital_out, [4-5]=dac0, [6-7]=dac1
///   Risposta: [0-1]=digital_in, [2-3]=digital_out_echo, [4-19]=adc[0..7]
///
/// Esecuzione:
///   cargo test --test selftest -- --ignored          # tutti i test hw
///   cargo test --test selftest gpio -- --ignored     # solo GPIO
///   cargo test --test selftest analog -- --ignored   # solo analogico
///   PHYCMD_PORT=/dev/ttyACM0 cargo test --test selftest -- --ignored

use std::io::{Read, Write};
use std::time::{Duration, Instant};

const MSG_SIZE: usize = 64;
const DAC_MAX: u16 = 4095;
const ADC_TOLERANCE: u16 = 80;
const ADC_NOISE_MAX_STDDEV: f64 = 20.0;
const GPIO_SETTLE_ROUNDS: usize = 3;

/// Risposta raw dal firmware
#[derive(Debug, Clone)]
struct FwStatus {
    digital_in: u16,
    digital_out: u16,
    adc: [u16; 8],
}

/// Client raw per comunicazione diretta con firmware
struct FwClient {
    port: Box<dyn serialport::SerialPort>,
}

impl FwClient {
    fn open() -> Self {
        let port_name = std::env::var("PHYCMD_PORT").unwrap_or_else(|_| {
            // Auto-detect: cerca porte ACM o usbmodem
            let ports = serialport::available_ports().expect("Impossibile enumerare porte seriali");
            for p in &ports {
                if p.port_name.contains("ACM") || p.port_name.contains("usbmodem") {
                    return p.port_name.clone();
                }
                if let serialport::SerialPortType::UsbPort(usb) = &p.port_type {
                    // Atmel/Arduino VID
                    if usb.vid == 0x2341 || usb.vid == 0x03EB {
                        return p.port_name.clone();
                    }
                }
            }
            panic!(
                "Nessun device PhyCommander trovato. Imposta PHYCMD_PORT.\nPorte disponibili: {:?}",
                ports.iter().map(|p| &p.port_name).collect::<Vec<_>>()
            );
        });

        let port = serialport::new(&port_name, 921600)
            .timeout(Duration::from_millis(500))
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .flow_control(serialport::FlowControl::None)
            .open()
            .unwrap_or_else(|e| panic!("Impossibile aprire {}: {}", port_name, e));

        eprintln!("[selftest] Connesso a {}", port_name);

        let mut client = Self { port };
        // Flush iniziale
        let _ = client.port.clear(serialport::ClearBuffer::All);
        client
    }

    fn exchange(&mut self, digital_out: u16, dac0: u16, dac1: u16) -> FwStatus {
        let mut buf = [0u8; MSG_SIZE];
        // Header (compatibile physerver, firmware lo ignora)
        buf[0] = 0x55;
        buf[1] = 0xAA;
        // digital_out
        buf[2] = (digital_out & 0xFF) as u8;
        buf[3] = (digital_out >> 8) as u8;
        // dac0
        let dac0 = dac0.min(DAC_MAX);
        buf[4] = (dac0 & 0xFF) as u8;
        buf[5] = (dac0 >> 8) as u8;
        // dac1
        let dac1 = dac1.min(DAC_MAX);
        buf[6] = (dac1 & 0xFF) as u8;
        buf[7] = (dac1 >> 8) as u8;

        self.port.write_all(&buf).expect("Errore scrittura seriale");
        self.port.flush().expect("Errore flush seriale");

        let mut resp = [0u8; MSG_SIZE];
        self.port
            .read_exact(&mut resp)
            .expect("Timeout lettura risposta");

        let digital_in = u16::from_le_bytes([resp[0], resp[1]]);
        let digital_out = u16::from_le_bytes([resp[2], resp[3]]);
        let mut adc = [0u16; 8];
        for i in 0..8 {
            adc[i] = u16::from_le_bytes([resp[4 + i * 2], resp[5 + i * 2]]);
        }

        FwStatus {
            digital_in,
            digital_out,
            adc,
        }
    }

    /// Invia comando e attendi stabilizzazione
    fn exchange_settle(&mut self, digital_out: u16, dac0: u16, dac1: u16) -> FwStatus {
        let mut status = self.exchange(digital_out, dac0, dac1);
        for _ in 1..GPIO_SETTLE_ROUNDS {
            status = self.exchange(digital_out, dac0, dac1);
        }
        status
    }

    /// Campiona ADC con media su N letture
    fn read_adc_avg(&mut self, dac0: u16, dac1: u16, samples: usize) -> [f64; 8] {
        // Imposta DAC e attendi settling
        self.exchange(0, dac0, dac1);
        std::thread::sleep(Duration::from_millis(5));

        let mut sums = [0.0f64; 8];
        for _ in 0..samples {
            let status = self.exchange(0, dac0, dac1);
            for ch in 0..8 {
                sums[ch] += status.adc[ch] as f64;
            }
        }
        for ch in 0..8 {
            sums[ch] /= samples as f64;
        }
        sums
    }
}

// ============================================================
//  Test comunicazione
// ============================================================

#[test]
#[ignore] // Richiede hardware connesso
fn selftest_communication() {
    let mut dev = FwClient::open();

    // Scambio base
    let status = dev.exchange(0, 0, 0);
    eprintln!("[comm] Risposta ricevuta: {:?}", status);

    // Echo digital_out
    let status = dev.exchange(0xA5A5, 0, 0);
    assert_eq!(
        status.digital_out, 0xA5A5,
        "Echo digital_out errato: 0x{:04X}",
        status.digital_out
    );

    // 100 scambi consecutivi
    for i in 0u16..100 {
        let status = dev.exchange(i, 0, 0);
        assert_eq!(
            status.digital_out, i,
            "Echo errato allo scambio {}: atteso {}, ricevuto {}",
            i, i, status.digital_out
        );
    }

    // Misura latenza
    let mut latencies = Vec::with_capacity(50);
    for _ in 0..50 {
        let t0 = Instant::now();
        dev.exchange(0, 0, 0);
        latencies.push(t0.elapsed().as_micros() as f64 / 1000.0);
    }
    let avg: f64 = latencies.iter().sum::<f64>() / latencies.len() as f64;
    let min = latencies.iter().cloned().fold(f64::MAX, f64::min);
    let max = latencies.iter().cloned().fold(0.0f64, f64::max);
    eprintln!(
        "[comm] Latenza: media={:.1}ms  min={:.1}ms  max={:.1}ms",
        avg, min, max
    );
}

// ============================================================
//  Test GPIO loopback
// ============================================================

#[test]
#[ignore]
fn selftest_gpio_all_zero() {
    let mut dev = FwClient::open();
    let status = dev.exchange_settle(0x0000, 0, 0);
    assert_eq!(
        status.digital_in, 0x0000,
        "GPIO tutti-0: DIN=0x{:04X}",
        status.digital_in
    );
}

#[test]
#[ignore]
fn selftest_gpio_all_one() {
    let mut dev = FwClient::open();
    let status = dev.exchange_settle(0xFFFF, 0, 0);
    assert_eq!(
        status.digital_in, 0xFFFF,
        "GPIO tutti-1: DIN=0x{:04X}",
        status.digital_in
    );
}

#[test]
#[ignore]
fn selftest_gpio_walking_ones() {
    let mut dev = FwClient::open();
    for bit in 0..16u16 {
        let pattern = 1u16 << bit;
        let status = dev.exchange_settle(pattern, 0, 0);
        assert_eq!(
            status.digital_in, pattern,
            "Walking-1 bit {}: DOUT=0x{:04X}, DIN=0x{:04X}",
            bit, pattern, status.digital_in
        );
    }
}

#[test]
#[ignore]
fn selftest_gpio_walking_zeros() {
    let mut dev = FwClient::open();
    for bit in 0..16u16 {
        let pattern = 0xFFFFu16 & !(1u16 << bit);
        let status = dev.exchange_settle(pattern, 0, 0);
        assert_eq!(
            status.digital_in, pattern,
            "Walking-0 bit {}: DOUT=0x{:04X}, DIN=0x{:04X}",
            bit, pattern, status.digital_in
        );
    }
}

#[test]
#[ignore]
fn selftest_gpio_patterns() {
    let mut dev = FwClient::open();
    let patterns: &[u16] = &[0xAAAA, 0x5555, 0xFF00, 0x00FF, 0xF0F0, 0x0F0F];
    for &pattern in patterns {
        let status = dev.exchange_settle(pattern, 0, 0);
        assert_eq!(
            status.digital_in, pattern,
            "Pattern 0x{:04X}: DIN=0x{:04X}",
            pattern, status.digital_in
        );
    }
    // Ripristina
    dev.exchange(0, 0, 0);
}

// ============================================================
//  Test DAC/ADC loopback analogico
// ============================================================

#[test]
#[ignore]
fn selftest_analog_zero_scale() {
    let mut dev = FwClient::open();
    let avg = dev.read_adc_avg(0, 0, 10);
    assert!(
        avg[0] < ADC_TOLERANCE as f64,
        "Zero scale ADC0={:.0} (max {})",
        avg[0], ADC_TOLERANCE
    );
    assert!(
        avg[1] < ADC_TOLERANCE as f64,
        "Zero scale ADC1={:.0} (max {})",
        avg[1], ADC_TOLERANCE
    );
    eprintln!("[analog] Zero: ADC0={:.1}, ADC1={:.1}", avg[0], avg[1]);
}

#[test]
#[ignore]
fn selftest_analog_full_scale() {
    let mut dev = FwClient::open();
    let avg = dev.read_adc_avg(DAC_MAX, DAC_MAX, 10);
    let threshold = (DAC_MAX - ADC_TOLERANCE) as f64;
    assert!(
        avg[0] > threshold,
        "Full scale ADC0={:.0} (min {:.0})",
        avg[0], threshold
    );
    assert!(
        avg[1] > threshold,
        "Full scale ADC1={:.0} (min {:.0})",
        avg[1], threshold
    );
    eprintln!("[analog] Full: ADC0={:.1}, ADC1={:.1}", avg[0], avg[1]);
}

#[test]
#[ignore]
fn selftest_analog_mid_scale() {
    let mut dev = FwClient::open();
    let mid = DAC_MAX / 2;
    let avg = dev.read_adc_avg(mid, mid, 10);
    let error0 = (avg[0] - mid as f64).abs();
    let error1 = (avg[1] - mid as f64).abs();
    assert!(
        error0 < ADC_TOLERANCE as f64,
        "Mid scale ADC0={:.0}, errore={:.0} (max {})",
        avg[0], error0, ADC_TOLERANCE
    );
    assert!(
        error1 < ADC_TOLERANCE as f64,
        "Mid scale ADC1={:.0}, errore={:.0} (max {})",
        avg[1], error1, ADC_TOLERANCE
    );
    eprintln!(
        "[analog] Mid (DAC={}): ADC0={:.1} (err={:.1}), ADC1={:.1} (err={:.1})",
        mid, avg[0], error0, avg[1], error1
    );
}

#[test]
#[ignore]
fn selftest_analog_linearity_ramp() {
    let mut dev = FwClient::open();
    let steps = 16;
    let step_size = DAC_MAX / steps;
    let mut max_error: f64 = 0.0;

    eprintln!("[analog] Rampa linearita DAC0->ADC0 ({} punti):", steps + 1);
    for i in 0..=steps {
        let dac_val = (i * step_size).min(DAC_MAX);
        let avg = dev.read_adc_avg(dac_val, 0, 5);
        let error = (avg[0] - dac_val as f64).abs();
        max_error = max_error.max(error);

        eprintln!(
            "  DAC={:4} ({:.3}V) -> ADC={:6.1} ({:.3}V)  err={:.1}",
            dac_val,
            dac_val as f64 / DAC_MAX as f64 * 3.3,
            avg[0],
            avg[0] / DAC_MAX as f64 * 3.3,
            error
        );
    }
    assert!(
        max_error < ADC_TOLERANCE as f64,
        "Errore linearita max={:.0} (tolleranza {})",
        max_error, ADC_TOLERANCE
    );
    // Ripristina
    dev.exchange(0, 0, 0);
}

#[test]
#[ignore]
fn selftest_analog_noise() {
    let mut dev = FwClient::open();
    let mid = DAC_MAX / 2;
    // Settling
    dev.exchange(0, mid, mid);
    std::thread::sleep(Duration::from_millis(10));

    let samples = 100;
    let mut ch0_vals = Vec::with_capacity(samples);
    let mut ch1_vals = Vec::with_capacity(samples);

    for _ in 0..samples {
        let status = dev.exchange(0, mid, mid);
        ch0_vals.push(status.adc[0] as f64);
        ch1_vals.push(status.adc[1] as f64);
    }

    fn stddev(vals: &[f64]) -> (f64, f64, f64, f64) {
        let avg = vals.iter().sum::<f64>() / vals.len() as f64;
        let var = vals.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / vals.len() as f64;
        let min = vals.iter().cloned().fold(f64::MAX, f64::min);
        let max = vals.iter().cloned().fold(f64::MIN, f64::max);
        (avg, var.sqrt(), min, max)
    }

    let (avg0, std0, min0, max0) = stddev(&ch0_vals);
    let (avg1, std1, min1, max1) = stddev(&ch1_vals);

    eprintln!(
        "[noise] ADC0: media={:.1}, stddev={:.1}, range=[{:.0},{:.0}]",
        avg0, std0, min0, max0
    );
    eprintln!(
        "[noise] ADC1: media={:.1}, stddev={:.1}, range=[{:.0},{:.0}]",
        avg1, std1, min1, max1
    );

    assert!(
        std0 < ADC_NOISE_MAX_STDDEV,
        "Rumore ADC0 eccessivo: stddev={:.1} (max {})",
        std0, ADC_NOISE_MAX_STDDEV
    );
    assert!(
        std1 < ADC_NOISE_MAX_STDDEV,
        "Rumore ADC1 eccessivo: stddev={:.1} (max {})",
        std1, ADC_NOISE_MAX_STDDEV
    );
    // Ripristina
    dev.exchange(0, 0, 0);
}

#[test]
#[ignore]
fn selftest_analog_channel_independence() {
    let mut dev = FwClient::open();

    // DAC0=max, DAC1=0
    let avg_a = dev.read_adc_avg(DAC_MAX, 0, 10);
    // DAC0=0, DAC1=max
    let avg_b = dev.read_adc_avg(0, DAC_MAX, 10);

    // Quando DAC0=max: ADC0 deve essere alto, ADC1 basso
    assert!(avg_a[0] > (DAC_MAX - ADC_TOLERANCE) as f64, "ADC0 basso con DAC0=max");
    assert!(avg_a[1] < ADC_TOLERANCE as f64, "Crosstalk: ADC1 alto con DAC0=max");

    // Quando DAC1=max: ADC0 basso, ADC1 alto
    assert!(avg_b[0] < ADC_TOLERANCE as f64, "Crosstalk: ADC0 alto con DAC1=max");
    assert!(avg_b[1] > (DAC_MAX - ADC_TOLERANCE) as f64, "ADC1 basso con DAC1=max");

    eprintln!("[crosstalk] DAC0=max: ADC0={:.0}, ADC1={:.0}", avg_a[0], avg_a[1]);
    eprintln!("[crosstalk] DAC1=max: ADC0={:.0}, ADC1={:.0}", avg_b[0], avg_b[1]);

    // Ripristina
    dev.exchange(0, 0, 0);
}

// ============================================================
//  Test onda quadra GPIO
// ============================================================

#[test]
#[ignore]
fn selftest_square_wave_max_frequency() {
    let mut dev = FwClient::open();
    dev.exchange(0, 0, 0);

    let num_cycles = 200;
    let total_samples = num_cycles * 2;
    let mut timestamps = Vec::with_capacity(total_samples);
    let mut states_in = Vec::with_capacity(total_samples);

    for i in 0..total_samples {
        let out = if i % 2 == 0 { 0x0001u16 } else { 0x0000u16 };
        let status = dev.exchange(out, 0, 0);
        timestamps.push(Instant::now());
        states_in.push(status.digital_in & 1);
    }

    let total_time = timestamps.last().unwrap().duration_since(timestamps[0]);
    let total_secs = total_time.as_secs_f64();
    let freq_out = num_cycles as f64 / total_secs;

    // Conta transizioni input
    let transitions: usize = states_in
        .windows(2)
        .filter(|w| w[0] != w[1])
        .count();
    let freq_in = (transitions as f64 / 2.0) / total_secs;

    eprintln!(
        "[square] Max freq: generata={:.1}Hz, misurata={:.1}Hz, transizioni={}",
        freq_out, freq_in, transitions
    );

    // La freq misurata puo' differire per il ritardo di un campione
    assert!(
        transitions > 0,
        "Nessuna transizione rilevata su DIN0 (DOUT0->DIN0 connesso?)"
    );

    let error_pct = (freq_in - freq_out).abs() / freq_out * 100.0;
    assert!(
        error_pct < 15.0,
        "Errore frequenza {:.1}% (max 15%)",
        error_pct
    );

    // Ripristina
    dev.exchange(0, 0, 0);
}

#[test]
#[ignore]
fn selftest_square_wave_target_frequencies() {
    let mut dev = FwClient::open();
    dev.exchange(0, 0, 0);

    let target_freqs = [10.0, 25.0, 50.0, 100.0, 200.0]; // Hz

    for &target_hz in &target_freqs {
        let half_period = Duration::from_secs_f64(1.0 / (2.0 * target_hz));
        let num_cycles = (target_hz as usize).clamp(10, 100);
        let total_samples = num_cycles * 2;

        let mut timestamps = Vec::with_capacity(total_samples);
        let mut states_in = Vec::with_capacity(total_samples);

        for i in 0..total_samples {
            let out = if i % 2 == 0 { 0x0001u16 } else { 0x0000u16 };
            let status = dev.exchange(out, 0, 0);
            timestamps.push(Instant::now());
            states_in.push(status.digital_in & 1);

            if i < total_samples - 1 {
                let elapsed = timestamps.last().unwrap().elapsed();
                if elapsed < half_period {
                    std::thread::sleep(half_period - elapsed);
                }
            }
        }

        let total_secs = timestamps
            .last()
            .unwrap()
            .duration_since(timestamps[0])
            .as_secs_f64();

        let transitions: usize = states_in
            .windows(2)
            .filter(|w| w[0] != w[1])
            .count();

        if transitions > 0 && total_secs > 0.0 {
            let measured_hz = (transitions as f64 / 2.0) / total_secs;
            let error_pct = (measured_hz - target_hz).abs() / target_hz * 100.0;

            eprintln!(
                "[square] {}Hz: misurata={:.1}Hz, errore={:.1}%",
                target_hz, measured_hz, error_pct
            );

            assert!(
                error_pct < 20.0,
                "Onda quadra {}Hz: misurata={:.1}Hz (errore {:.1}%, max 20%)",
                target_hz, measured_hz, error_pct
            );
        } else {
            panic!(
                "Onda quadra {}Hz: nessuna transizione rilevata",
                target_hz
            );
        }
    }

    // Ripristina
    dev.exchange(0, 0, 0);
}

#[test]
#[ignore]
fn selftest_square_wave_analog() {
    let mut dev = FwClient::open();

    let num_cycles = 50;
    let mut adc_highs = Vec::with_capacity(num_cycles);
    let mut adc_lows = Vec::with_capacity(num_cycles);

    for i in 0..(num_cycles * 2) {
        if i % 2 == 0 {
            dev.exchange(0, DAC_MAX, 0);
            std::thread::sleep(Duration::from_millis(2));
            let status = dev.exchange(0, DAC_MAX, 0);
            adc_highs.push(status.adc[0] as f64);
        } else {
            dev.exchange(0, 0, 0);
            std::thread::sleep(Duration::from_millis(2));
            let status = dev.exchange(0, 0, 0);
            adc_lows.push(status.adc[0] as f64);
        }
    }

    let avg_high = adc_highs.iter().sum::<f64>() / adc_highs.len() as f64;
    let avg_low = adc_lows.iter().sum::<f64>() / adc_lows.len() as f64;
    let amplitude = avg_high - avg_low;

    eprintln!(
        "[square_analog] DAC0->ADC0: high={:.0}, low={:.0}, ampiezza={:.0} ({:.2}V)",
        avg_high, avg_low, amplitude,
        amplitude / DAC_MAX as f64 * 3.3
    );

    assert!(
        amplitude > DAC_MAX as f64 * 0.8,
        "Ampiezza onda quadra analogica insufficiente: {:.0} (min {:.0})",
        amplitude, DAC_MAX as f64 * 0.8
    );

    // Ripristina
    dev.exchange(0, 0, 0);
}
