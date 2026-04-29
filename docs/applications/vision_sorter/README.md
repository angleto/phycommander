# Vision Sorter (Coral + 2-axis encoder arm) (concept)

> A desktop-scale optical sorting line. A USB webcam over a slope or short
> conveyor sees small objects passing under it; a Google Coral USB Edge TPU
> classifies them in real time; a 2-axis encoder arm picks each object and
> drops it in the correct bin based on the predicted class. Industrial
> parallel: vision-based sorting in waste, food, and pharma production
> lines.

---

## 1. TL;DR

Webcam frames are pulled by OpenCV on the host. A quantized image classifier
runs on the Coral USB accelerator at ~30 FPS. When an object is detected
above a confidence threshold, the host queues a pick-and-place sequence: it
sends position setpoints to a Python PID loop running at 1 kHz that reads
the two encoders (decoded from PhyCommander's 8 kHz DIN stream) and drives
two PWM channels through an external H-bridge. The arm picks, moves to the
bin corresponding to the class label, drops, and returns home.

The narrative point of the demo is the iteration speed of the perception
side. Adding a new class is `dataset/, retrain in colab, scp model.tflite,
rerun the notebook`. Zero firmware involvement.

---

## 2. Why this demo (vs a bare Arduino sketch)

What Python on the host makes possible:
- TensorFlow Lite + `pycoral` inference on a USB-attached TPU. The Cortex-M3
  in the Due has neither the memory nor the throughput for any serious
  model.
- OpenCV preprocessing, annotation overlays, in-place dataset capture for
  retraining.
- Behaviour orchestration: a finite state machine with retries, timeouts,
  failure modes, and metrics, in roughly 100 lines of Python.

What PhyCommander adds on top:
- 8 kHz DIN sampling lets the host decode quadrature encoders in software
  with no missed counts up to several thousand encoder edges per second
  (printer-salvage motors with 100 to 500 cpr at < 1000 RPM stay well
  inside this budget).
- Streaming Command frames at 8 kHz let the Python PID loop close at 1 kHz
  with jitter dominated by the iso-USB scheduler (about 8 µs std dev on the
  reference EHCI host), independent of Python GC pauses, because the
  firmware always has the most recent PWM duty in flight.

What a bare Arduino sketch cannot reasonably do: any of the perception
side, period.

### A note on PID location

The current PhyCommander on-chip PID (`play_pid`, see
[`../../user-guide/FUNCTION_GENERATOR.md`](../../user-guide/FUNCTION_GENERATOR.md)
§3.4) accepts an ADC channel as input, not encoder counts. For
encoder-driven position the loop runs in **Python at 1 kHz**, which is well
within the determinism budget of the streaming Command frame. Firmware-side
TC quadrature decode is on the project roadmap (see
[`../LOCKIN_OPTICAL_DEMO.md`](../LOCKIN_OPTICAL_DEMO.md) §6) and would let
the loop migrate on-chip later without changing the host API.

---

## 3. Hardware bill of materials

Parts the operator already has, plus minor add-ons.

| Qty | Item | Notes |
|---|---|---|
| 1 | Webcam, USB UVC, 720p+ at 30 FPS | any consumer model |
| 1 | Google Coral USB Accelerator | Edge TPU |
| 2 | DC motor with quadrature encoder (printer salvage) | already on hand |
| 1 | Dual H-bridge driver (L298, DRV8833, or similar DIP/module) | drives the two motors from PWM0 and PWM1 |
| 1 | Mechanical 2-axis frame | the operator's existing 2-motor stack |
| 1 | End effector (cardboard pusher, or simple compliant gripper) | low-effort first iteration |
| 1 | Small slope or short conveyor | even an inclined cardboard slide works |
| 2 to 4 | Bins (paper cups, small boxes) | one per class |
| 1 | Light source over the camera (LED ring or desk lamp) | consistent illumination is the largest single source of accuracy |

---

## 4. Mapping to phycommander I/O

| Channel | Mode | Role |
|---|---|---|
| **PWM0** | manual, duty driven by host PID at 1 kHz | motor 1 (axis A) drive |
| **PWM1** | manual, duty driven by host PID at 1 kHz | motor 2 (axis B) drive |
| **DOUT0** | manual | motor 1 direction (H-bridge IN1) |
| **DOUT1** | manual | motor 2 direction (H-bridge IN1) |
| **DOUT2** | manual | LED ring power, lit during measurement |
| **DOUT15** | reactive `play_threshold` on ADC0 | hard cutoff if motor 1 stall current is exceeded, host-independent safety |
| **DIN0** | encoder A, motor 1 | quadrature, software-decoded on host from 8 kHz stream |
| **DIN1** | encoder B, motor 1 | idem |
| **DIN2** | encoder A, motor 2 | idem |
| **DIN3** | encoder B, motor 2 | idem |
| **DIN4** | end-stop motor 1 | homing |
| **DIN5** | end-stop motor 2 | homing |
| **DIN6** | photogate at pickup zone | software-debounced, triggers detect cycle |
| **ADC0** | motor 1 current sense | feeds DOUT15 stall-cutoff |
| **ADC1** | motor 2 current sense | logged |

---

## 5. Host software architecture

```
+----------------------------+
|  GUI / notebook            |   class labels overlay, bin counters
+----------------------------+
|  fsm.py                    |   IDLE -> DETECT -> MOVE_TO_PICK ->
|                            |   PICK -> MOVE_TO_BIN_X -> DROP -> IDLE
+----------------------------+
|  perception.py             |   webcam -> opencv crop -> coral inference
|  motion.py                 |   encoder decode + 1 kHz host PID, trapezoidal moves
+----------------------------+
|  phycmd (PyO3 client)      |   set_pwm0/1, set_dout, subscribe(on_frame)
+----------------------------+
```

Dependencies: `phycmd`, `pycoral` or `tflite-runtime`, `opencv-python`,
`numpy`.

A small TFLite model is required. Two sourcing options:
- 50 to 100 hand-collected images per class, trained on a laptop in
  ~20 minutes; quantize to int8 and recompile for Edge TPU.
- Fine-tune from a public dataset (TACO, WasteNet) for 1 to 3 epochs.

---

## 6. Video plan

Target: 3 to 4 minutes.

1. **Cold open (10 s)**: overhead shot of the table. A hand drops three
   different small objects on the slope. The Coral overlay annotates each
   as it passes. The arm picks them and drops them in different bins.
2. **Per-frame perception (45 s)**: inset the live OpenCV window with class
   probabilities. Show frame rate (>20 FPS).
3. **The iteration sell (60 s)**: operator decides to add a fourth class.
   Cuts to: capture 30 photos with a one-line script, retrain on Colab in
   2 minutes, scp the new `.tflite`, restart the notebook. The sorter now
   handles the new class. Off-screen elapsed: about 5 minutes; on-screen
   compressed to 60 s.
4. **PID live-tune (45 s)**: operator opens an `ipywidgets` slider in the
   notebook, drags Kp up and watches the arm overshoot the return-to-home.
   Drags it back. The change is live; no recompile.
5. **Punchline (15 s)**.

---

## 7. Validation experiments

| # | Experiment | Pass criterion |
|---|---|---|
| 1 | Per-class accuracy on a held-out test of 20 to 30 objects | overall accuracy > 90 % |
| 2 | Cycle time | average detect-to-drop < 4 s |
| 3 | Stall handling | 100 cycles with zero motor stall events not handled by the threshold safety |
| 4 | Repeatability of pick position | encoder-reported pick position varies less than 1 % of axis travel over 100 cycles |

---

## 8. Roadmap and extensions

- Add a third class (e.g. plastic, glass, metal) and retune the bin map.
- Add a second downward camera for grip-quality verification post-pick.
- Migrate the PID to firmware once `v1.2` exposes the TC peripheral (this
  is the same TC exposure listed in
  [`../LOCKIN_OPTICAL_DEMO.md`](../LOCKIN_OPTICAL_DEMO.md) §6).
- Online fine-tuning: every misclassified object is logged with its frame
  for periodic retraining.

---

## 9. Safety notes

- **Mechanical**: the 2-axis arm should have soft limits enforced both in
  the host motion module and as a DOUT-driven hardware mute (DOUT15
  reactive threshold on stall current, as mapped above). A runaway must be
  stoppable from the firmware-side hardware path, not just by killing
  Python.
- **Pinch points**: the gripper is the obvious one; the demo intentionally
  uses a pusher in the first iteration to avoid this entirely.

---

## 10. References

- [`../../user-guide/FUNCTION_GENERATOR.md`](../../user-guide/FUNCTION_GENERATOR.md)
  §3.2, §3.4: `play_threshold` for the stall safety, `play_pid` for future
  firmware-side closed-loop.
- [`../../firmware/PROTOCOL.md`](../../firmware/PROTOCOL.md): DIN sampling
  and Command frame format.
- [`../LOCKIN_OPTICAL_DEMO.md`](../LOCKIN_OPTICAL_DEMO.md) §6: TC
  peripheral exposure roadmap, relevant for hardware quadrature decode.
- [`../../../physerver/crates/phycmd-py/README.md`](../../../physerver/crates/phycmd-py/README.md):
  Python client API.

---

*Document version: concept draft 1, 2026-04-27. Status: concept. Implementation
will land under `applications/vision_sorter/` when started.*
