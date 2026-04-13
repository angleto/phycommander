#!/usr/bin/env python3
"""
PhyCommander on-chip function generator — Python helper.

Self-contained ctypes+libusb client for every vendor SETUP request
defined in docs/firmware/PROTOCOL.md.  No external Python deps.

Run on the host where the Arduino Due is plugged in.  Stop physerver
first if it has the device claimed:

    sudo systemctl stop physerver

Usage examples:

    python3 phycmd_waveform.py caps
    python3 phycmd_waveform.py state dac0
    python3 phycmd_waveform.py sine    dac0 1000  --amp 4000 --off 2048
    python3 phycmd_waveform.py square  dac0 5000  --duty 0.25
    python3 phycmd_waveform.py arb     dac0 csv:waveform.csv 100000
    python3 phycmd_waveform.py lut     dac0 adc 0  csv:transfer.csv
    python3 phycmd_waveform.py thresh  dout5 adc 0  --high 2100 --low 2000 --vh 1 --vl 0
    python3 phycmd_waveform.py pulse   dout6 din 0  --duration_us 100000 --cooldown_us 50000
    python3 phycmd_waveform.py pid     dac0 adc 0  --setpoint 2048 --kp 0.5 --ki 0.001
    python3 phycmd_waveform.py stop    dac0
    python3 phycmd_waveform.py dacclock 500000
    python3 phycmd_waveform.py adcrate  100000

Channel name conventions:
    dacN  →  channel id 0..1
    pwmN  →  channel id 2..9
    doutN →  channel id 10..25  (note: bit number = N, channel id = 10 + N)
    dinN  →  channel id 26..41
    adcN  →  channel id 42..49

The on-chip generator runs entirely on the SAM3X.  Once you've
issued PLAY_*, the host can disconnect and the device keeps producing
the waveform until it gets STOP, GEN_PLAY_* (replaced), or USB
disconnect (which auto-stops everything for safety, see PROTOCOL.md
§4.3).
"""
from __future__ import annotations

import argparse
import ctypes
import math
import struct
import sys
from pathlib import Path

# --- libusb-1.0 control transfer wrappers (ctypes, no third-party deps) -----

VID = 0x2341
PID = 0x003E
INTERFACE = 0

# Vendor SETUP request opcodes — must match waveform.h
VREQ_GEN_GET_CAPS         = 0x10
VREQ_GEN_GET_STATE        = 0x11
VREQ_GEN_PLAY_BUILTIN     = 0x12
VREQ_GEN_PLAY_ARBITRARY   = 0x13
VREQ_GEN_STOP             = 0x14
VREQ_DAC_SET_CLOCK        = 0x18
VREQ_DAC_GET_CLOCK        = 0x19
VREQ_ADC_SET_RATE         = 0x20
VREQ_ADC_GET_RATE         = 0x21
VREQ_GEN_PLAY_LUT         = 0x30
VREQ_GEN_PLAY_THRESHOLD   = 0x31
VREQ_GEN_PLAY_PULSE_TRIG  = 0x32
VREQ_GEN_PLAY_PID         = 0x38

# Shape codes
SHAPE_OFF, SHAPE_DC, SHAPE_SINE, SHAPE_SQUARE, SHAPE_TRIANGLE, \
    SHAPE_SAWTOOTH, SHAPE_ARBITRARY = range(7)
SHAPE_LUT, SHAPE_THRESHOLD, SHAPE_PULSE_TRIG, SHAPE_PID = 16, 17, 18, 19
SHAPE_NAMES = {
    0:'OFF', 1:'DC', 2:'SINE', 3:'SQUARE', 4:'TRIANGLE',
    5:'SAWTOOTH', 6:'ARBITRARY',
    16:'LUT', 17:'THRESHOLD', 18:'PULSE_TRIG', 19:'PID',
}

# Input source for reactive modes
INPUT_SRC_NONE, INPUT_SRC_ADC, INPUT_SRC_DIN_MASK = 0, 1, 2

# Channel ID layout (must match waveform.h channel_decode)
def channel_id(name: str) -> int:
    name = name.lower()
    kinds = [
        ('dac',   2),
        ('pwm',   8),
        ('dout', 16),
        ('din',  16),
        ('adc',   8),
    ]
    base = 0
    for kind, count in kinds:
        if name.startswith(kind):
            try:
                idx = int(name[len(kind):])
            except ValueError:
                raise ValueError(f"channel name {name!r} has no numeric index")
            if idx < 0 or idx >= count:
                raise ValueError(f"{name}: index {idx} out of 0..{count-1}")
            return base + idx
        base += count
    raise ValueError(f"unknown channel kind in {name!r}")


class Device:
    def __init__(self):
        self.lib = ctypes.CDLL("libusb-1.0.so.0")
        self.lib.libusb_control_transfer.argtypes = [
            ctypes.c_void_p, ctypes.c_uint8, ctypes.c_uint8,
            ctypes.c_uint16, ctypes.c_uint16, ctypes.c_void_p,
            ctypes.c_uint16, ctypes.c_uint
        ]
        self.lib.libusb_control_transfer.restype = ctypes.c_int
        self.lib.libusb_init(None)
        self.dh = self.lib.libusb_open_device_with_vid_pid(None, VID, PID)
        if not self.dh:
            raise RuntimeError(
                f"Arduino Due not found (VID={VID:04x} PID={PID:04x}). "
                f"Is physerver holding the interface?  `sudo systemctl stop physerver`")
        # detach kernel driver if any (best-effort)
        self.lib.libusb_detach_kernel_driver(self.dh, INTERFACE)
        rc = self.lib.libusb_claim_interface(self.dh, INTERFACE)
        if rc != 0:
            raise RuntimeError(f"claim_interface failed: {rc}")

    def close(self):
        if getattr(self, 'dh', None):
            self.lib.libusb_release_interface(self.dh, INTERFACE)
            self.lib.libusb_close(self.dh)
            self.dh = None
            self.lib.libusb_exit(None)

    def __enter__(self): return self
    def __exit__(self, *a): self.close()

    # ---- raw control transfer wrappers ----
    def _ctrl_in(self, bRequest, wValue=0, wIndex=0, length=32) -> bytes:
        buf = (ctypes.c_uint8 * length)()
        n = self.lib.libusb_control_transfer(
            self.dh, 0xC0, bRequest, wValue, wIndex, buf, length, 1000)
        if n < 0:
            raise RuntimeError(f"control_transfer IN bRequest=0x{bRequest:02x} failed: {n}")
        return bytes(buf[:n])

    def _ctrl_out(self, bRequest, wValue=0, wIndex=0, data: bytes = b""):
        buf_t = ctypes.c_uint8 * len(data) if data else ctypes.c_uint8 * 0
        buf = buf_t.from_buffer_copy(data) if data else None
        n = self.lib.libusb_control_transfer(
            self.dh, 0x40, bRequest, wValue, wIndex,
            buf, len(data), 1000)
        if n < 0:
            raise RuntimeError(f"control_transfer OUT bRequest=0x{bRequest:02x} failed: {n}")
        return n

    # ---- typed helpers ----
    def get_caps(self) -> dict:
        b = self._ctrl_in(VREQ_GEN_GET_CAPS, length=32)
        f = struct.unpack("<BBHHHBBBBBBBBBBBBBBBBII", b)
        return dict(
            protocol_version=f[0], firmware_minor=f[2], firmware_major=f[3],
            num_dac=f[5], num_pwm=f[6], num_dout=f[7], num_din=f[8], num_adc=f[9],
            modes_dac=f[13], modes_pwm=f[14], modes_dout=f[15],
            modes_din=f[16], modes_adc=f[17],
            max_dac_sample_rate_hz=f[21], max_arb_buffer_samples=f[22],
        )

    def get_state(self, channel: str) -> dict:
        b = self._ctrl_in(VREQ_GEN_GET_STATE, wIndex=channel_id(channel), length=32)
        f = struct.unpack("<BBBBIHHHHHHIII", b)
        return dict(
            channel_kind=f[0], channel_index=f[1], shape=f[2], shape_name=SHAPE_NAMES.get(f[2], '?'),
            flags=f[3], freq_mHz=f[4], duty_x10=f[5],
            amplitude=f[6], offset=f[7], phase_offset_x16=f[8],
            arb_n_samples=f[9], arb_loops_remaining=f[10],
            arb_sample_rate_hz=f[11], cur_phase_q24_8=f[12],
        )

    def stop(self, channel: str):
        self._ctrl_out(VREQ_GEN_STOP, wIndex=channel_id(channel))

    def dac_set_clock(self, hz: int):
        self._ctrl_out(VREQ_DAC_SET_CLOCK, data=struct.pack("<I", int(hz)))
    def dac_get_clock(self) -> int:
        return struct.unpack("<I", self._ctrl_in(VREQ_DAC_GET_CLOCK, length=4))[0]
    def adc_set_rate(self, hz: int):
        self._ctrl_out(VREQ_ADC_SET_RATE, data=struct.pack("<I", int(hz)))
    def adc_get_rate(self) -> int:
        return struct.unpack("<I", self._ctrl_in(VREQ_ADC_GET_RATE, length=4))[0]

    def play_builtin(self, channel: str, shape: int, *, freq_hz: float,
                     amplitude: int, offset: int, duty: float = 0.5):
        spec = struct.pack("<BBHHHIHH",
            shape, 0, int(round(duty * 1000)), int(amplitude), int(offset),
            int(round(freq_hz * 1000)), 0, 0)
        self._ctrl_out(VREQ_GEN_PLAY_BUILTIN, wIndex=channel_id(channel), data=spec)

    def play_arbitrary(self, channel: str, samples: list[int],
                       sample_rate_hz: int, loop_count: int = 0):
        if len(samples) > 1024:
            raise ValueError("max 1024 samples per arbitrary buffer")
        header = struct.pack("<HHI", len(samples), int(loop_count), int(sample_rate_hz))
        body = b"".join(struct.pack("<h", int(s)) for s in samples)
        self._ctrl_out(VREQ_GEN_PLAY_ARBITRARY, wIndex=channel_id(channel),
                       data=header + body)

    def play_lut(self, channel: str, input_src: str, input_arg: int,
                 entries: list[int], output_mask: int = 0):
        src = INPUT_SRC_ADC if input_src == 'adc' else INPUT_SRC_DIN_MASK
        if len(entries) == 0 or len(entries) > 4096:
            raise ValueError("entries must be 1..4096")
        header = struct.pack("<BBHHHI", src, 0, int(input_arg), len(entries),
                             int(output_mask), 0)
        body = b"".join(struct.pack("<h", int(e)) for e in entries)
        self._ctrl_out(VREQ_GEN_PLAY_LUT, wIndex=channel_id(channel),
                       data=header + body)

    def play_threshold(self, channel: str, input_src: str, input_arg: int, *,
                       thr_high: int, thr_low: int, val_high: int, val_low: int):
        src = INPUT_SRC_ADC if input_src == 'adc' else INPUT_SRC_DIN_MASK
        spec = struct.pack("<BBHHHHHI", src, 0, int(input_arg),
                           int(thr_high), int(thr_low),
                           int(val_high), int(val_low), 0)
        self._ctrl_out(VREQ_GEN_PLAY_THRESHOLD, wIndex=channel_id(channel), data=spec)

    def play_pulse_trig(self, channel: str, din_bit: int, *, edge: str = 'rising',
                        active_level: int = 1, duration_us: int = 1000,
                        cooldown_us: int = 0):
        edge_v = {'rising': 1, 'falling': 2, 'any': 3}[edge]
        spec = struct.pack("<BBBBII", int(din_bit), edge_v, int(active_level), 0,
                           int(duration_us), int(cooldown_us))
        self._ctrl_out(VREQ_GEN_PLAY_PULSE_TRIG, wIndex=channel_id(channel), data=spec)

    def play_pid(self, channel: str, input_src: str, input_arg: int, *,
                 sample_rate_hz: int = 1000, setpoint: int,
                 kp: float, ki: float = 0.0, kd: float = 0.0,
                 out_min: int = 0, out_max: int = 4095,
                 integral_clamp: int = 1_000_000):
        src = INPUT_SRC_ADC if input_src == 'adc' else INPUT_SRC_DIN_MASK
        spec = struct.pack("<BBHIiiiiHHi", src, 0, int(input_arg),
                           int(sample_rate_hz), int(setpoint),
                           int(kp * 65536), int(ki * 65536), int(kd * 65536),
                           int(out_min), int(out_max), int(integral_clamp))
        self._ctrl_out(VREQ_GEN_PLAY_PID, wIndex=channel_id(channel), data=spec)


# --- helpers for samples loading -------------------------------------------

def load_samples(spec: str) -> list[int]:
    """`csv:path` reads one int per line.  Otherwise treats `spec` as a
    comma-separated list."""
    if spec.startswith('csv:'):
        path = Path(spec[4:])
        return [int(line.strip()) for line in path.read_text().splitlines() if line.strip()]
    return [int(x.strip()) for x in spec.split(',') if x.strip()]


# --- CLI ------------------------------------------------------------------

def fmt_modes(mask: int) -> str:
    names = ['MANUAL', 'BUILTIN', 'ARBITRARY', 'LUT', 'THRESHOLD', 'PULSE_TRIG', 'PID', 'RULE']
    return '|'.join(n for i, n in enumerate(names) if mask & (1 << i)) or '-'


def main():
    p = argparse.ArgumentParser(description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter)
    sp = p.add_subparsers(dest='cmd', required=True)

    sp.add_parser('caps')
    s = sp.add_parser('state'); s.add_argument('channel')
    s = sp.add_parser('stop');  s.add_argument('channel')

    s = sp.add_parser('dacclock'); s.add_argument('hz', type=int, nargs='?')
    s = sp.add_parser('adcrate');  s.add_argument('hz', type=int, nargs='?')

    for shape in ('dc', 'sine', 'square', 'triangle', 'sawtooth'):
        s = sp.add_parser(shape)
        s.add_argument('channel')
        s.add_argument('freq_hz', type=float, nargs='?', default=1000.0)
        s.add_argument('--amp',  type=int, default=4000)
        s.add_argument('--off',  type=int, default=2048)
        s.add_argument('--duty', type=float, default=0.5)

    s = sp.add_parser('arb')
    s.add_argument('channel'); s.add_argument('samples'); s.add_argument('sample_rate_hz', type=int)
    s.add_argument('--loops', type=int, default=0)

    s = sp.add_parser('lut')
    s.add_argument('channel'); s.add_argument('input_src', choices=['adc','din'])
    s.add_argument('input_arg', type=lambda x: int(x, 0))
    s.add_argument('entries')
    s.add_argument('--out_mask', type=lambda x: int(x, 0), default=0)

    s = sp.add_parser('thresh')
    s.add_argument('channel'); s.add_argument('input_src', choices=['adc','din'])
    s.add_argument('input_arg', type=lambda x: int(x, 0))
    s.add_argument('--high', type=int, required=True); s.add_argument('--low', type=int, required=True)
    s.add_argument('--vh', type=int, required=True); s.add_argument('--vl', type=int, required=True)

    s = sp.add_parser('pulse')
    s.add_argument('channel'); s.add_argument('input_src', choices=['din'])
    s.add_argument('din_bit', type=int)
    s.add_argument('--edge', choices=['rising','falling','any'], default='rising')
    s.add_argument('--active_level', type=int, default=1)
    s.add_argument('--duration_us', type=int, default=1000)
    s.add_argument('--cooldown_us', type=int, default=0)

    s = sp.add_parser('pid')
    s.add_argument('channel'); s.add_argument('input_src', choices=['adc'])
    s.add_argument('input_arg', type=lambda x: int(x, 0))
    s.add_argument('--setpoint', type=int, required=True)
    s.add_argument('--kp', type=float, required=True)
    s.add_argument('--ki', type=float, default=0.0)
    s.add_argument('--kd', type=float, default=0.0)
    s.add_argument('--out_min', type=int, default=0)
    s.add_argument('--out_max', type=int, default=4095)
    s.add_argument('--sample_rate_hz', type=int, default=1000)

    args = p.parse_args()

    with Device() as dev:
        if args.cmd == 'caps':
            c = dev.get_caps()
            print(f"protocol_version: {c['protocol_version']}, firmware {c['firmware_major']}.{c['firmware_minor']}")
            print(f"channels: DAC={c['num_dac']} PWM={c['num_pwm']} DOUT={c['num_dout']} DIN={c['num_din']} ADC={c['num_adc']}")
            print(f"modes:")
            print(f"  DAC  : 0x{c['modes_dac']:02x} = {fmt_modes(c['modes_dac'])}")
            print(f"  PWM  : 0x{c['modes_pwm']:02x} = {fmt_modes(c['modes_pwm'])}")
            print(f"  DOUT : 0x{c['modes_dout']:02x} = {fmt_modes(c['modes_dout'])}")
            print(f"max DAC sample rate: {c['max_dac_sample_rate_hz']} Hz")
            print(f"max arbitrary buf : {c['max_arb_buffer_samples']} samples")
        elif args.cmd == 'state':
            s = dev.get_state(args.channel)
            print(f"channel {args.channel}: kind={s['channel_kind']} idx={s['channel_index']}")
            print(f"  shape : {s['shape']} ({s['shape_name']})")
            if s['shape'] != SHAPE_OFF:
                print(f"  freq  : {s['freq_mHz']/1000:.3f} Hz   amp={s['amplitude']}  off={s['offset']}  duty={s['duty_x10']/10:.1f}%")
                print(f"  arb_n : {s['arb_n_samples']}  loops_left={s['arb_loops_remaining']}")
                print(f"  phase : 0x{s['cur_phase_q24_8']:08x} (Q24.8)")
        elif args.cmd == 'stop':
            dev.stop(args.channel); print("stopped")
        elif args.cmd == 'dacclock':
            if args.hz is None: print(f"DAC clock: {dev.dac_get_clock()} Hz")
            else: dev.dac_set_clock(args.hz); print(f"DAC clock set: {dev.dac_get_clock()} Hz")
        elif args.cmd == 'adcrate':
            if args.hz is None: print(f"ADC rate: {dev.adc_get_rate()} Hz")
            else: dev.adc_set_rate(args.hz); print(f"ADC rate set: {dev.adc_get_rate()} Hz")
        elif args.cmd in ('dc','sine','square','triangle','sawtooth'):
            shape_map = {'dc': SHAPE_DC, 'sine': SHAPE_SINE, 'square': SHAPE_SQUARE,
                         'triangle': SHAPE_TRIANGLE, 'sawtooth': SHAPE_SAWTOOTH}
            dev.play_builtin(args.channel, shape_map[args.cmd],
                             freq_hz=args.freq_hz, amplitude=args.amp,
                             offset=args.off, duty=args.duty)
            print(f"playing {args.cmd} on {args.channel}: freq={args.freq_hz} Hz amp={args.amp} off={args.off}")
        elif args.cmd == 'arb':
            samples = load_samples(args.samples)
            dev.play_arbitrary(args.channel, samples, args.sample_rate_hz, args.loops)
            print(f"playing arbitrary on {args.channel}: {len(samples)} samples @ {args.sample_rate_hz} Hz, loops={args.loops}")
        elif args.cmd == 'lut':
            entries = load_samples(args.entries)
            dev.play_lut(args.channel, args.input_src, args.input_arg, entries, args.out_mask)
            print(f"LUT armed on {args.channel}: input={args.input_src}({args.input_arg}), {len(entries)} entries, out_mask=0x{args.out_mask:04x}")
        elif args.cmd == 'thresh':
            dev.play_threshold(args.channel, args.input_src, args.input_arg,
                               thr_high=args.high, thr_low=args.low,
                               val_high=args.vh, val_low=args.vl)
            print(f"THRESHOLD armed on {args.channel}: hi={args.high}/{args.vh} lo={args.low}/{args.vl}")
        elif args.cmd == 'pulse':
            dev.play_pulse_trig(args.channel, args.din_bit, edge=args.edge,
                                active_level=args.active_level,
                                duration_us=args.duration_us,
                                cooldown_us=args.cooldown_us)
            print(f"PULSE_TRIG armed on {args.channel}: din{args.din_bit} {args.edge} → {args.duration_us} µs")
        elif args.cmd == 'pid':
            dev.play_pid(args.channel, args.input_src, args.input_arg,
                         sample_rate_hz=args.sample_rate_hz,
                         setpoint=args.setpoint, kp=args.kp, ki=args.ki, kd=args.kd,
                         out_min=args.out_min, out_max=args.out_max)
            print(f"PID armed on {args.channel}: setpoint={args.setpoint} Kp={args.kp} Ki={args.ki} Kd={args.kd}")


if __name__ == '__main__':
    sys.exit(main() or 0)
