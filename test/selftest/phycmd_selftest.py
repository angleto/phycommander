#!/usr/bin/env python3
"""
PhyCommander Hardware Self-Test Suite
=====================================

Auto-test per il campionamento analogico/digitale e generazione onde quadre.
Richiede connessioni loopback fisiche tra le porte del device.

Connessioni richieste:
  ANALOGICO:
    DAC0 (pin DAC0) --> ADC ch0 (pin A0)
    DAC1 (pin DAC1) --> ADC ch1 (pin A1)

  DIGITALE (GPIO loopback):
    DOUT0  --> DIN0       DOUT8  --> DIN8
    DOUT1  --> DIN1       DOUT9  --> DIN9
    DOUT2  --> DIN2       DOUT10 --> DIN10
    DOUT3  --> DIN3       DOUT11 --> DIN11
    DOUT4  --> DIN4       DOUT12 --> DIN12
    DOUT5  --> DIN5       DOUT13 --> DIN13
    DOUT6  --> DIN6       DOUT14 --> DIN14
    DOUT7  --> DIN7       DOUT15 --> DIN15

  ONDA QUADRA (frequenza):
    DOUT0 --> DIN0  (stesso loopback, usato per misura frequenza)

Protocollo firmware (raw, 64 byte):
  Comando (host->device):
    [0-1]  ignorati (header physerver, compatibile)
    [2-3]  digital_out (u16 LE)
    [4-5]  dac0 (u16 LE, 0-4095)
    [6-7]  dac1 (u16 LE, 0-4095)
    [8-63] riservati

  Risposta (device->host):
    [0-1]  digital_in (u16 LE)
    [2-3]  digital_out echo (u16 LE)
    [4-19] ADC ch0-ch7 (8 x u16 LE)
    [20-63] non definiti

Uso:
  python3 phycmd_selftest.py                        # auto-detect porta
  python3 phycmd_selftest.py -p /dev/ttyACM0        # porta specifica
  python3 phycmd_selftest.py -t gpio                # solo test GPIO
  python3 phycmd_selftest.py -t analog              # solo test analogico
  python3 phycmd_selftest.py -t square_wave         # solo test onda quadra
  python3 phycmd_selftest.py -v                     # output verboso
"""

import argparse
import struct
import sys
import time
from dataclasses import dataclass
from typing import Optional

try:
    import serial
    import serial.tools.list_ports
except ImportError:
    print("ERRORE: pyserial non installato. Esegui: pip3 install pyserial")
    sys.exit(1)

# --- Costanti protocollo ---
MSG_SIZE = 64
DAC_MAX = 4095       # 12-bit DAC
ADC_MAX = 4095       # 12-bit ADC
ADC_CHANNELS = 8
GPIO_PINS = 16
VREF = 3.3           # Tensione di riferimento (V)

# Tolleranze
ADC_TOLERANCE_COUNTS = 80    # ~65mV a 3.3V, margine per rumore + offset
ADC_NOISE_MAX_STDDEV = 20    # max deviazione standard accettabile (counts)
GPIO_SETTLE_EXCHANGES = 3    # scambi per stabilizzazione GPIO


@dataclass
class DeviceStatus:
    """Risposta dal firmware."""
    digital_in: int       # u16, stato GPIO input
    digital_out: int      # u16, echo GPIO output
    adc: list             # [u16] x 8, valori ADC


class PhyCmdDevice:
    """Comunicazione diretta con il firmware PhyCommander."""

    def __init__(self, port: str, timeout: float = 0.5):
        self.ser = serial.Serial(
            port=port,
            parity=serial.PARITY_NONE,
            stopbits=serial.STOPBITS_ONE,
            bytesize=serial.EIGHTBITS,
            timeout=timeout,
        )
        # Flush buffer iniziale
        self.ser.reset_input_buffer()
        self.ser.reset_output_buffer()

    def close(self):
        if self.ser and self.ser.is_open:
            self.ser.close()

    def exchange(self, digital_out: int = 0, dac0: int = 0, dac1: int = 0) -> DeviceStatus:
        """Invia comando e ricevi risposta. Protocollo firmware raw."""
        buf = bytearray(MSG_SIZE)
        # [0-1] header (compatibile con physerver, firmware li ignora)
        struct.pack_into('<H', buf, 0, 0xAA55)
        # [2-3] digital_out
        struct.pack_into('<H', buf, 2, digital_out & 0xFFFF)
        # [4-5] dac0
        struct.pack_into('<H', buf, 4, min(dac0, DAC_MAX))
        # [6-7] dac1
        struct.pack_into('<H', buf, 6, min(dac1, DAC_MAX))

        self.ser.write(buf)
        self.ser.flush()

        resp = self.ser.read(MSG_SIZE)
        if len(resp) != MSG_SIZE:
            raise TimeoutError(
                f"Risposta incompleta: {len(resp)}/{MSG_SIZE} byte"
            )

        # Parse risposta firmware
        digital_in = struct.unpack_from('<H', resp, 0)[0]
        digital_out_echo = struct.unpack_from('<H', resp, 2)[0]
        adc = list(struct.unpack_from('<8H', resp, 4))

        return DeviceStatus(
            digital_in=digital_in,
            digital_out=digital_out_echo,
            adc=adc,
        )


# --- Risultati test ---
class TestResult:
    def __init__(self, name: str):
        self.name = name
        self.passed = 0
        self.failed = 0
        self.details = []

    def ok(self, msg: str):
        self.passed += 1
        self.details.append(('PASS', msg))

    def fail(self, msg: str):
        self.failed += 1
        self.details.append(('FAIL', msg))

    @property
    def success(self) -> bool:
        return self.failed == 0

    def print_summary(self):
        status = "PASS" if self.success else "FAIL"
        print(f"\n{'='*60}")
        print(f"  [{status}] {self.name}  ({self.passed} ok, {self.failed} errori)")
        print(f"{'='*60}")
        for kind, msg in self.details:
            marker = "  [+]" if kind == 'PASS' else "  [!]"
            print(f"{marker} {msg}")


# --- Test comunicazione ---
def test_communication(dev: PhyCmdDevice, verbose: bool) -> TestResult:
    """Verifica che il device risponda correttamente."""
    result = TestResult("Comunicazione seriale")

    # Test 1: scambio base
    try:
        status = dev.exchange(digital_out=0, dac0=0, dac1=0)
        result.ok("Scambio comando/risposta OK")
    except Exception as e:
        result.fail(f"Scambio fallito: {e}")
        return result

    # Test 2: echo digital_out
    try:
        status = dev.exchange(digital_out=0xA5A5)
        if status.digital_out == 0xA5A5:
            result.ok("Echo digital_out corretto (0xA5A5)")
        else:
            result.fail(
                f"Echo digital_out errato: atteso 0xA5A5, ricevuto 0x{status.digital_out:04X}"
            )
    except Exception as e:
        result.fail(f"Echo test fallito: {e}")

    # Test 3: scambi multipli consecutivi
    try:
        errors = 0
        for i in range(100):
            val = i & 0xFFFF
            status = dev.exchange(digital_out=val)
            if status.digital_out != val:
                errors += 1
        if errors == 0:
            result.ok("100 scambi consecutivi senza errori")
        else:
            result.fail(f"{errors}/100 scambi con echo errato")
    except Exception as e:
        result.fail(f"Test scambi multipli fallito: {e}")

    # Test 4: misura latenza
    try:
        latencies = []
        for _ in range(50):
            t0 = time.perf_counter()
            dev.exchange()
            t1 = time.perf_counter()
            latencies.append((t1 - t0) * 1000)  # ms
        avg = sum(latencies) / len(latencies)
        mn = min(latencies)
        mx = max(latencies)
        result.ok(f"Latenza: media={avg:.1f}ms  min={mn:.1f}ms  max={mx:.1f}ms")
    except Exception as e:
        result.fail(f"Test latenza fallito: {e}")

    return result


# --- Test GPIO loopback ---
def test_gpio_loopback(dev: PhyCmdDevice, verbose: bool) -> TestResult:
    """Test loopback digitale: DOUT[n] -> DIN[n] per n=0..15."""
    result = TestResult("GPIO Loopback (DOUT -> DIN)")

    def settle_and_read(digital_out: int) -> int:
        """Invia il valore e attendi stabilizzazione, ritorna digital_in."""
        status = None
        for _ in range(GPIO_SETTLE_EXCHANGES):
            status = dev.exchange(digital_out=digital_out)
        return status.digital_in

    # Test 1: tutti a 0
    try:
        din = settle_and_read(0x0000)
        if din == 0x0000:
            result.ok("Tutti GPIO a 0: OK")
        else:
            result.fail(f"Tutti GPIO a 0: DIN=0x{din:04X} (atteso 0x0000)")
    except Exception as e:
        result.fail(f"Test tutti-0 fallito: {e}")
        return result

    # Test 2: tutti a 1
    try:
        din = settle_and_read(0xFFFF)
        if din == 0xFFFF:
            result.ok("Tutti GPIO a 1: OK")
        else:
            result.fail(f"Tutti GPIO a 1: DIN=0x{din:04X} (atteso 0xFFFF)")
            # Identifica pin non connessi
            for bit in range(GPIO_PINS):
                if not (din & (1 << bit)):
                    result.fail(f"  Pin {bit}: non risponde (DOUT{bit}->DIN{bit} scollegato?)")
    except Exception as e:
        result.fail(f"Test tutti-1 fallito: {e}")

    # Test 3: walking ones (un bit alla volta)
    try:
        walk_errors = 0
        for bit in range(GPIO_PINS):
            pattern = 1 << bit
            din = settle_and_read(pattern)
            if din != pattern:
                walk_errors += 1
                result.fail(
                    f"Walking-1 bit {bit}: DOUT=0x{pattern:04X}, DIN=0x{din:04X}"
                )
                if verbose:
                    # Analisi dettagliata: bit stuck o corto
                    for b in range(GPIO_PINS):
                        expected = 1 if b == bit else 0
                        actual = (din >> b) & 1
                        if expected != actual:
                            if actual:
                                print(f"    DIN{b} stuck-at-1 o corto con DIN{bit}")
                            else:
                                print(f"    DIN{bit} stuck-at-0 o scollegato")
        if walk_errors == 0:
            result.ok("Walking ones (16 bit): tutti OK")
    except Exception as e:
        result.fail(f"Walking ones fallito: {e}")

    # Test 4: walking zeros
    try:
        walk_errors = 0
        for bit in range(GPIO_PINS):
            pattern = 0xFFFF & ~(1 << bit)
            din = settle_and_read(pattern)
            if din != pattern:
                walk_errors += 1
                result.fail(
                    f"Walking-0 bit {bit}: DOUT=0x{pattern:04X}, DIN=0x{din:04X}"
                )
        if walk_errors == 0:
            result.ok("Walking zeros (16 bit): tutti OK")
    except Exception as e:
        result.fail(f"Walking zeros fallito: {e}")

    # Test 5: pattern alternati
    try:
        patterns = [0xAAAA, 0x5555, 0xFF00, 0x00FF, 0xF0F0, 0x0F0F]
        pat_errors = 0
        for pattern in patterns:
            din = settle_and_read(pattern)
            if din != pattern:
                pat_errors += 1
                result.fail(
                    f"Pattern 0x{pattern:04X}: DIN=0x{din:04X}"
                )
        if pat_errors == 0:
            result.ok(f"Pattern alternati ({len(patterns)} pattern): tutti OK")
    except Exception as e:
        result.fail(f"Pattern test fallito: {e}")

    # Ripristina output a 0
    dev.exchange(digital_out=0)
    return result


# --- Test DAC/ADC loopback ---
def test_analog_loopback(dev: PhyCmdDevice, verbose: bool) -> TestResult:
    """Test loopback analogico: DAC0->ADC0, DAC1->ADC1."""
    result = TestResult("Analogico DAC/ADC Loopback")

    def read_adc_stable(dac0: int = 0, dac1: int = 0, samples: int = 10) -> list:
        """Imposta DAC e campiona ADC multiplo per valore stabile."""
        # Primo scambio per impostare DAC
        dev.exchange(dac0=dac0, dac1=dac1)
        # Attendi stabilizzazione (settling time DAC + ADC)
        time.sleep(0.005)
        # Campiona
        readings = []
        for _ in range(samples):
            status = dev.exchange(dac0=dac0, dac1=dac1)
            readings.append(status.adc[:2])  # Solo ch0 e ch1
        return readings

    # --- Test 1: Zero scale ---
    try:
        readings = read_adc_stable(dac0=0, dac1=0)
        avg_ch0 = sum(r[0] for r in readings) / len(readings)
        avg_ch1 = sum(r[1] for r in readings) / len(readings)
        if avg_ch0 < ADC_TOLERANCE_COUNTS and avg_ch1 < ADC_TOLERANCE_COUNTS:
            result.ok(f"Zero scale: ADC0={avg_ch0:.0f}, ADC1={avg_ch1:.0f} (< {ADC_TOLERANCE_COUNTS})")
        else:
            if avg_ch0 >= ADC_TOLERANCE_COUNTS:
                result.fail(f"Zero scale ADC0={avg_ch0:.0f} troppo alto (max {ADC_TOLERANCE_COUNTS})")
            if avg_ch1 >= ADC_TOLERANCE_COUNTS:
                result.fail(f"Zero scale ADC1={avg_ch1:.0f} troppo alto (max {ADC_TOLERANCE_COUNTS})")
    except Exception as e:
        result.fail(f"Test zero scale fallito: {e}")
        return result

    # --- Test 2: Full scale ---
    try:
        readings = read_adc_stable(dac0=DAC_MAX, dac1=DAC_MAX)
        avg_ch0 = sum(r[0] for r in readings) / len(readings)
        avg_ch1 = sum(r[1] for r in readings) / len(readings)
        threshold = DAC_MAX - ADC_TOLERANCE_COUNTS
        if avg_ch0 > threshold and avg_ch1 > threshold:
            result.ok(f"Full scale: ADC0={avg_ch0:.0f}, ADC1={avg_ch1:.0f} (> {threshold})")
        else:
            if avg_ch0 <= threshold:
                result.fail(f"Full scale ADC0={avg_ch0:.0f} troppo basso (min {threshold})")
            if avg_ch1 <= threshold:
                result.fail(f"Full scale ADC1={avg_ch1:.0f} troppo basso (min {threshold})")
    except Exception as e:
        result.fail(f"Test full scale fallito: {e}")

    # --- Test 3: Mid scale ---
    try:
        mid = DAC_MAX // 2
        readings = read_adc_stable(dac0=mid, dac1=mid)
        avg_ch0 = sum(r[0] for r in readings) / len(readings)
        avg_ch1 = sum(r[1] for r in readings) / len(readings)
        if abs(avg_ch0 - mid) < ADC_TOLERANCE_COUNTS and abs(avg_ch1 - mid) < ADC_TOLERANCE_COUNTS:
            result.ok(
                f"Mid scale (DAC={mid}): ADC0={avg_ch0:.0f}, ADC1={avg_ch1:.0f} "
                f"(errore: {abs(avg_ch0 - mid):.0f}, {abs(avg_ch1 - mid):.0f})"
            )
        else:
            result.fail(
                f"Mid scale: ADC0={avg_ch0:.0f} ADC1={avg_ch1:.0f} "
                f"(atteso ~{mid}, tolleranza {ADC_TOLERANCE_COUNTS})"
            )
    except Exception as e:
        result.fail(f"Test mid scale fallito: {e}")

    # --- Test 4: Rampa di linearita (solo DAC0->ADC0) ---
    try:
        steps = 16
        step_size = DAC_MAX // steps
        max_error = 0
        linearity_ok = True
        ramp_data = []

        for i in range(steps + 1):
            dac_val = min(i * step_size, DAC_MAX)
            readings = read_adc_stable(dac0=dac_val, dac1=0, samples=5)
            avg = sum(r[0] for r in readings) / len(readings)
            error = abs(avg - dac_val)
            max_error = max(max_error, error)
            ramp_data.append((dac_val, avg, error))

            if verbose:
                voltage_dac = (dac_val / DAC_MAX) * VREF
                voltage_adc = (avg / ADC_MAX) * VREF
                print(
                    f"    DAC0={dac_val:4d} ({voltage_dac:.3f}V) -> "
                    f"ADC0={avg:6.1f} ({voltage_adc:.3f}V)  err={error:.1f}"
                )

        if max_error < ADC_TOLERANCE_COUNTS:
            result.ok(
                f"Rampa linearita DAC0->ADC0 ({steps+1} punti): "
                f"errore max={max_error:.1f} counts"
            )
        else:
            result.fail(
                f"Rampa linearita: errore max={max_error:.1f} counts "
                f"(tolleranza {ADC_TOLERANCE_COUNTS})"
            )
            # Mostra i punti peggiori
            worst = sorted(ramp_data, key=lambda x: x[2], reverse=True)[:3]
            for dac_val, adc_val, err in worst:
                result.fail(f"  DAC={dac_val} -> ADC={adc_val:.0f} (err={err:.0f})")
    except Exception as e:
        result.fail(f"Test rampa fallito: {e}")

    # --- Test 5: Rumore ADC (stabilita statica) ---
    try:
        mid = DAC_MAX // 2
        readings = read_adc_stable(dac0=mid, dac1=mid, samples=100)
        ch0_vals = [r[0] for r in readings]
        ch1_vals = [r[1] for r in readings]

        def calc_stats(vals):
            avg = sum(vals) / len(vals)
            variance = sum((v - avg) ** 2 for v in vals) / len(vals)
            stddev = variance ** 0.5
            return avg, stddev, min(vals), max(vals)

        avg0, std0, min0, max0 = calc_stats(ch0_vals)
        avg1, std1, min1, max1 = calc_stats(ch1_vals)

        if std0 < ADC_NOISE_MAX_STDDEV:
            result.ok(
                f"Rumore ADC0: stddev={std0:.1f}  range=[{min0},{max0}]  "
                f"({((max0-min0)/ADC_MAX)*VREF*1000:.1f}mV pk-pk)"
            )
        else:
            result.fail(
                f"Rumore ADC0 eccessivo: stddev={std0:.1f} (max {ADC_NOISE_MAX_STDDEV})"
            )

        if std1 < ADC_NOISE_MAX_STDDEV:
            result.ok(
                f"Rumore ADC1: stddev={std1:.1f}  range=[{min1},{max1}]  "
                f"({((max1-min1)/ADC_MAX)*VREF*1000:.1f}mV pk-pk)"
            )
        else:
            result.fail(
                f"Rumore ADC1 eccessivo: stddev={std1:.1f} (max {ADC_NOISE_MAX_STDDEV})"
            )
    except Exception as e:
        result.fail(f"Test rumore fallito: {e}")

    # --- Test 6: Indipendenza canali ---
    try:
        # DAC0 a fondo scala, DAC1 a zero
        readings = read_adc_stable(dac0=DAC_MAX, dac1=0, samples=10)
        avg_ch0 = sum(r[0] for r in readings) / len(readings)
        avg_ch1 = sum(r[1] for r in readings) / len(readings)

        ch0_ok = avg_ch0 > (DAC_MAX - ADC_TOLERANCE_COUNTS)
        ch1_ok = avg_ch1 < ADC_TOLERANCE_COUNTS

        # DAC0 a zero, DAC1 a fondo scala
        readings = read_adc_stable(dac0=0, dac1=DAC_MAX, samples=10)
        avg_ch0b = sum(r[0] for r in readings) / len(readings)
        avg_ch1b = sum(r[1] for r in readings) / len(readings)

        ch0b_ok = avg_ch0b < ADC_TOLERANCE_COUNTS
        ch1b_ok = avg_ch1b > (DAC_MAX - ADC_TOLERANCE_COUNTS)

        if ch0_ok and ch1_ok and ch0b_ok and ch1b_ok:
            result.ok("Indipendenza canali DAC0/DAC1: OK (nessun crosstalk)")
        else:
            result.fail(
                f"Crosstalk rilevato: "
                f"DAC0=max->ADC0={avg_ch0:.0f},ADC1={avg_ch1:.0f} | "
                f"DAC1=max->ADC0={avg_ch0b:.0f},ADC1={avg_ch1b:.0f}"
            )
    except Exception as e:
        result.fail(f"Test indipendenza canali fallito: {e}")

    # Ripristina DAC a 0
    dev.exchange(dac0=0, dac1=0)
    return result


# --- Test onda quadra ---
def test_square_wave(dev: PhyCmdDevice, verbose: bool) -> TestResult:
    """
    Test generazione e misura onda quadra via GPIO loopback.

    Genera onde quadre su DOUT[0] togglando il pin ad ogni scambio,
    e misura la frequenza risultante campionando DIN[0].
    La frequenza massima e' limitata dal round-trip USB (~1ms = ~500Hz).

    Test aggiuntivo: genera onde quadre a frequenze target (con delay)
    e verifica che la frequenza misurata sia entro tolleranza.
    """
    result = TestResult("Onda Quadra GPIO (frequenza)")

    # Azzera output
    dev.exchange(digital_out=0)
    time.sleep(0.01)

    # --- Test 1: Massima frequenza (toggle ad ogni scambio) ---
    try:
        num_cycles = 200
        timestamps = []
        states_out = []
        states_in = []

        for i in range(num_cycles * 2):
            out_val = 0x0001 if (i % 2 == 0) else 0x0000
            t0 = time.perf_counter()
            status = dev.exchange(digital_out=out_val)
            t1 = time.perf_counter()

            timestamps.append(t1)
            states_out.append(out_val & 1)
            states_in.append(status.digital_in & 1)

        # Calcola frequenza dal timing
        total_time = timestamps[-1] - timestamps[0]
        freq_out = num_cycles / total_time

        # Conta transizioni sull'input
        transitions_in = sum(
            1 for i in range(1, len(states_in)) if states_in[i] != states_in[i - 1]
        )
        freq_in = (transitions_in / 2) / total_time  # 2 transizioni = 1 ciclo

        result.ok(
            f"Max frequenza: generata={freq_out:.1f}Hz  "
            f"misurata={freq_in:.1f}Hz  transizioni={transitions_in}"
        )

        # Verifica coerenza (la freq misurata puo' essere leggermente diversa
        # per il ritardo di un campione tra output e lettura input)
        if freq_in > 0 and abs(freq_in - freq_out) / freq_out < 0.15:
            result.ok(f"Coerenza frequenza: errore={abs(freq_in-freq_out)/freq_out*100:.1f}%")
        elif freq_in > 0:
            result.fail(
                f"Frequenza incoerente: out={freq_out:.1f}Hz in={freq_in:.1f}Hz "
                f"(errore {abs(freq_in-freq_out)/freq_out*100:.1f}%)"
            )
    except Exception as e:
        result.fail(f"Test max frequenza fallito: {e}")

    # --- Test 2: Frequenze target con delay ---
    target_freqs = [10, 25, 50, 100, 200]  # Hz

    for target_hz in target_freqs:
        try:
            half_period = 1.0 / (2.0 * target_hz)  # secondi per semi-periodo
            num_cycles = max(10, min(target_hz, 100))  # almeno 10 cicli
            total_samples = num_cycles * 2

            timestamps = []
            states_in = []

            for i in range(total_samples):
                out_val = 0x0001 if (i % 2 == 0) else 0x0000
                status = dev.exchange(digital_out=out_val)
                timestamps.append(time.perf_counter())
                states_in.append(status.digital_in & 1)

                # Attendi per raggiungere la frequenza target
                # (sottrai tempo di scambio stimato)
                if i < total_samples - 1:
                    elapsed = time.perf_counter() - timestamps[-1]
                    sleep_time = half_period - elapsed
                    if sleep_time > 0:
                        time.sleep(sleep_time)

            # Calcola frequenza misurata
            total_time = timestamps[-1] - timestamps[0]
            transitions_in = sum(
                1 for j in range(1, len(states_in))
                if states_in[j] != states_in[j - 1]
            )
            if total_time > 0 and transitions_in > 0:
                measured_hz = (transitions_in / 2) / total_time
                error_pct = abs(measured_hz - target_hz) / target_hz * 100

                if error_pct < 20:  # 20% tolleranza (USB timing jitter)
                    result.ok(
                        f"Onda quadra {target_hz:3d}Hz: misurata={measured_hz:.1f}Hz "
                        f"(errore={error_pct:.1f}%)"
                    )
                else:
                    result.fail(
                        f"Onda quadra {target_hz:3d}Hz: misurata={measured_hz:.1f}Hz "
                        f"(errore={error_pct:.1f}%, max 20%)"
                    )
            else:
                result.fail(f"Onda quadra {target_hz}Hz: nessuna transizione rilevata")

        except Exception as e:
            result.fail(f"Test onda quadra {target_hz}Hz fallito: {e}")

    # --- Test 3: Duty cycle via DAC (onda quadra analogica) ---
    # Genera onda quadra sul DAC e verifica che l'ADC veda l'alternanza
    try:
        high_val = DAC_MAX
        low_val = 0
        num_cycles = 50
        adc_highs = []
        adc_lows = []

        for i in range(num_cycles * 2):
            if i % 2 == 0:
                status = dev.exchange(dac0=high_val)
                time.sleep(0.002)  # settling
                status = dev.exchange(dac0=high_val)
                adc_highs.append(status.adc[0])
            else:
                status = dev.exchange(dac0=low_val)
                time.sleep(0.002)
                status = dev.exchange(dac0=low_val)
                adc_lows.append(status.adc[0])

        avg_high = sum(adc_highs) / len(adc_highs) if adc_highs else 0
        avg_low = sum(adc_lows) / len(adc_lows) if adc_lows else 0
        amplitude = avg_high - avg_low

        if amplitude > (DAC_MAX * 0.8):
            result.ok(
                f"Onda quadra analogica DAC0->ADC0: "
                f"high={avg_high:.0f} low={avg_low:.0f} ampiezza={amplitude:.0f} counts "
                f"({(amplitude/ADC_MAX)*VREF:.2f}V)"
            )
        else:
            result.fail(
                f"Onda quadra analogica: ampiezza insufficiente={amplitude:.0f} "
                f"(atteso >{DAC_MAX*0.8:.0f})"
            )
    except Exception as e:
        result.fail(f"Test onda quadra analogica fallito: {e}")

    # Ripristina
    dev.exchange(digital_out=0, dac0=0, dac1=0)
    return result


# --- Test campionamento ADC multi-canale ---
def test_adc_sampling(dev: PhyCmdDevice, verbose: bool) -> TestResult:
    """Test dei canali ADC non collegati (ch2-ch7): verifica lettura e range."""
    result = TestResult("Campionamento ADC (tutti i canali)")

    try:
        # Campiona tutti i canali
        num_samples = 50
        all_readings = [[] for _ in range(ADC_CHANNELS)]

        for _ in range(num_samples):
            status = dev.exchange()
            for ch in range(ADC_CHANNELS):
                all_readings[ch].append(status.adc[ch])

        for ch in range(ADC_CHANNELS):
            vals = all_readings[ch]
            avg = sum(vals) / len(vals)
            mn, mx = min(vals), max(vals)
            variance = sum((v - avg) ** 2 for v in vals) / len(vals)
            stddev = variance ** 0.5

            in_range = all(0 <= v <= ADC_MAX for v in vals)
            if in_range:
                result.ok(
                    f"ADC ch{ch}: media={avg:6.1f}  range=[{mn},{mx}]  "
                    f"stddev={stddev:.1f}  ({(avg/ADC_MAX)*VREF:.3f}V)"
                )
            else:
                result.fail(f"ADC ch{ch}: valori fuori range [0,{ADC_MAX}]")

    except Exception as e:
        result.fail(f"Test campionamento ADC fallito: {e}")

    return result


# --- Auto-detect porta ---
def find_device_port() -> Optional[str]:
    """Cerca una porta seriale compatibile con PhyCommander."""
    ports = serial.tools.list_ports.comports()
    candidates = []
    for p in ports:
        # Cerca device USB CDC (ACM su Linux, usbmodem su macOS)
        if 'ACM' in p.device or 'usbmodem' in p.device:
            candidates.append(p.device)
        elif p.vid and p.pid:
            # Atmel/Arduino VID
            if p.vid in (0x2341, 0x03EB):
                candidates.append(p.device)

    if candidates:
        return candidates[0]

    # Fallback: mostra tutte le porte disponibili
    if ports:
        print("Porte seriali trovate (nessuna riconosciuta come PhyCMD):")
        for p in ports:
            print(f"  {p.device}  [{p.description}]")
    return None


# --- Main ---
def main():
    parser = argparse.ArgumentParser(
        description='PhyCommander Hardware Self-Test Suite',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        '-p', '--port',
        help='Porta seriale (es. /dev/ttyACM0, /dev/tty.usbmodemXXXX)',
    )
    parser.add_argument(
        '-t', '--test',
        choices=['all', 'comm', 'gpio', 'analog', 'square_wave', 'adc_scan'],
        default='all',
        help='Test da eseguire (default: all)',
    )
    parser.add_argument(
        '-v', '--verbose', action='store_true',
        help='Output verboso con dettagli campionamento',
    )
    args = parser.parse_args()

    # Trova porta
    port = args.port
    if not port:
        port = find_device_port()
        if not port:
            print("ERRORE: Nessun device PhyCommander trovato.")
            print("Specifica la porta con -p /dev/ttyACM0")
            sys.exit(1)

    print(f"""
{'='*60}
  PhyCommander Self-Test Suite
  Porta: {port}
{'='*60}
""")

    # Apri connessione
    try:
        dev = PhyCmdDevice(port)
    except Exception as e:
        print(f"ERRORE: Impossibile aprire {port}: {e}")
        sys.exit(1)

    results = []

    try:
        test_map = {
            'comm': lambda: test_communication(dev, args.verbose),
            'gpio': lambda: test_gpio_loopback(dev, args.verbose),
            'analog': lambda: test_analog_loopback(dev, args.verbose),
            'square_wave': lambda: test_square_wave(dev, args.verbose),
            'adc_scan': lambda: test_adc_sampling(dev, args.verbose),
        }

        if args.test == 'all':
            tests_to_run = ['comm', 'gpio', 'analog', 'square_wave', 'adc_scan']
        else:
            tests_to_run = [args.test]

        for test_name in tests_to_run:
            r = test_map[test_name]()
            r.print_summary()
            results.append(r)

    finally:
        dev.close()

    # Report finale
    total_pass = sum(r.passed for r in results)
    total_fail = sum(r.failed for r in results)
    all_ok = all(r.success for r in results)

    print(f"\n{'='*60}")
    print(f"  RISULTATO FINALE: {'PASS' if all_ok else 'FAIL'}")
    print(f"  Test superati: {total_pass}  Falliti: {total_fail}")
    print(f"{'='*60}\n")

    sys.exit(0 if all_ok else 1)


if __name__ == '__main__':
    main()
