# Laser Tracker (vision-driven pan-tilt) (concept)

> A pan-tilt mount built from two encoder DC motors carries a laser
> pointer. A fixed webcam tracks a target (face, ArUco marker, coloured
> blob); the host computes pan and tilt setpoints and the laser visibly
> follows the target. Industrial parallel: laser marking heads, optical
> alignment stages, weld-head positioning, telescope auto-guiding.

---

## 1. TL;DR

Two encoder DC motors form a 2-axis gimbal. A laser pointer is mounted on
the tilt axis and gated through a DOUT pin so a runaway Python script
cannot leave the laser on. A fixed USB webcam runs at 30 FPS; OpenCV
(or MediaPipe pose) returns a target pixel; a 3-point calibration maps
pixels to pan and tilt angles; a 1 kHz host PID loop drives the two PWM
channels using encoder counts decoded from PhyCommander's DIN stream.

The video sells two things at once: a satisfying physical effect (a red
dot chasing a hand) and the prototyping pitch (drag a slider, retune the
PID gains live, watch the dot stop overshooting, no firmware rebuild).

---

## 2. Why this demo (vs a bare Arduino sketch)

What Python on the host makes possible:
- OpenCV / MediaPipe vision; the Cortex-M3 has neither memory nor
  throughput for it.
- `ipywidgets` sliders for live PID tuning. The video sequence "drag
  slider, overshoot disappears" is the prototyping pitch in one shot.
- Kalman or alpha-beta smoothing of the target position to reject tracker
  jitter independently of the inner PID dynamics.

What PhyCommander adds on top:
- Determinism on the inner loop. The streaming Command frame at 8 kHz keeps
  the PWM duty current within 125 µs of the host PID output, independent
  of Python GC pauses.
- DOUT-gated laser enable that survives a host crash. Combined with a DIN
  cover-closed switch (per
  [`../LOCKIN_OPTICAL_DEMO.md`](../LOCKIN_OPTICAL_DEMO.md) §5.4) the laser
  cannot be driven on if the safety chain is broken, even when the host
  is misbehaving.

---

## 3. Hardware bill of materials

| Qty | Item | Notes |
|---|---|---|
| 1 | Webcam, USB UVC | 720p, 30 FPS |
| 2 | DC motor with quadrature encoder (printer salvage) | already on hand |
| 1 | Dual H-bridge module (L298, DRV8833, or similar) | from the same family used by the sorter demo |
| 1 | Pan-tilt frame (3D-printed or wood) | small base + tilt yoke |
| 1 | Laser pointer module, Class 1, < 1 mW, 650 nm | already on hand; verify class before using |
| 1 | Logic-level MOSFET (2N7000 or similar) | gates the laser on DOUT0 |
| 1 | Microswitch | cover-closed safety interlock |
| -  | Misc passives, wires | |

---

## 4. Mapping to phycommander I/O

| Channel | Mode | Role |
|---|---|---|
| **PWM0** | manual, duty from host PID | pan motor drive |
| **PWM1** | manual, duty from host PID | tilt motor drive |
| **DOUT0** | manual, host-controlled laser enable | gates the MOSFET that powers the laser |
| **DOUT1** | manual | pan motor direction |
| **DOUT2** | manual | tilt motor direction |
| **DOUT3** | reactive `play_threshold` on ADC0 | laser power monitor: if photodiode reads above limit, firmware pulls DOUT0 low without host intervention |
| **DIN0** / **DIN1** | encoder A / B, pan motor | quadrature, software-decoded on host |
| **DIN2** / **DIN3** | encoder A / B, tilt motor | idem |
| **DIN4** | cover-closed microswitch | host disables laser when open; firmware also gates DOUT0 via `play_pulse_trig` mirroring DIN4 (defence-in-depth) |
| **ADC0** | photodiode looking at the laser exit aperture | feeds the reactive threshold above |
| **ADC1** | motor current sense | logged for stall detection |

---

## 5. Host software architecture

```
+----------------------------+
|  notebook + ipywidgets     |   PID gains, target source, calibration
+----------------------------+
|  vision.py                 |   webcam capture -> face / ArUco / blob detect
|  calibration.py            |   pixel <-> angle mapping (3-point fit)
|  motion.py                 |   encoder decode + 1 kHz PID per axis
|  smoother.py               |   Kalman 1D for each angle setpoint
+----------------------------+
|  phycmd (PyO3 client)      |
+----------------------------+
```

Dependencies: `phycmd`, `opencv-python`, `numpy`. Optional: `mediapipe`
for pose-driven targeting; `ipywidgets` and `jupyter` for the live tuner.

---

## 6. Video plan

Target: 2 to 3 minutes. The bottleneck is making the laser dot visible on
camera; shoot in a darkened room with a wall as the screen.

1. **Cold open (10 s)**: laser dot on a wall. A hand enters frame, the dot
   follows. No annotation.
2. **Behind the camera (30 s)**: split screen, left is the wall shot,
   right is the OpenCV view with bounding box and angle setpoint.
3. **Live PID tuning (60 s)**: operator drags Kp up; the dot starts
   overshooting visibly. Drags it back; smooth tracking. Pulls Ki to
   zero; the dot lags. Restores. Each adjustment is one slider drag, no
   recompile.
4. **Stress test (30 s)**: a small thrown ball or yo-yo. The dot tries to
   keep up; latency becomes visible and is honestly shown.
5. **Punchline (15 s)**: operator opens `vision.py`, swaps face detect for
   ArUco marker detect, reruns the cell. The same gimbal now follows a
   printed marker. About 30 s of file edits on screen.

---

## 7. Validation experiments

| # | Experiment | Pass criterion |
|---|---|---|
| 1 | Step response | settling time < 200 ms for a 30° step, ≤ 5 % overshoot with the final tuned gains |
| 2 | Continuous tracking | p99 angular error < 2° for a target moving at about 0.5 m/s at 1 m distance |
| 3 | Safety chain | opening the cover switch (DIN4) extinguishes the laser within 1 ms regardless of host state, including with the host paused at a Python breakpoint |
| 4 | No runaway on host disconnect | unplug USB; both motors come to rest, the laser is off |

---

## 8. Roadmap and extensions

- Replace face detection with Coral pose estimation; target the operator's
  wrist instead of the face.
- Add a focus axis (third motor or a focus-tunable lens) for laser-marking
  use cases.
- Compute a small aimed pattern (square, circle, text) by feeding the PID
  a pre-recorded angle trajectory; this turns the demo into a galvo-style
  laser-marking show.

---

## 9. Safety notes

This demo involves a laser. Observe:

- **Use a Class 1 source** (under 1 mW for visible wavelengths). Standard
  red laser pointers are typically Class 2 or 3R; verify the class before
  using and attenuate if needed (neutral-density filter, or PWM-duty
  the laser and disclose it as averaged power).
- **Eye-safety chain**: the cover-closed switch on DIN4 must
  hardware-gate the laser through DOUT0 via the reactive `play_pulse_trig`
  or `play_threshold` slot, not only via host code. The host is not part
  of the safety case.
- **No reflective surfaces** in the laser path.
- **Stop button**: the demo binds the spacebar to a host stop that drops
  DOUT0; the cover switch is the authoritative stop.

---

## 10. References

- [`../../user-guide/FUNCTION_GENERATOR.md`](../../user-guide/FUNCTION_GENERATOR.md)
  §3.2 / §3.3: `play_threshold` and `play_pulse_trig` for the safety chain.
- [`../../firmware/PROTOCOL.md`](../../firmware/PROTOCOL.md): DOUT mask in
  the Command frame and DIN bits in the Status frame.
- [`../LOCKIN_OPTICAL_DEMO.md`](../LOCKIN_OPTICAL_DEMO.md) §5.4: laser
  interlock pattern.
- [`../../../physerver/crates/phycmd-py/README.md`](../../../physerver/crates/phycmd-py/README.md):
  Python client API.

---

*Document version: concept draft 1, 2026-04-27. Status: concept. Implementation
will land under `applications/laser_tracker/` when started.*
