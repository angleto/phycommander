# Companion Board

Hardware design for the phycommander **companion board**: the physical layer that adapts the Arduino Due microcontroller to the chassis front panel (8 of the 9 front D-Subs are routed by this board, the 9th is the host parallel port) and to the chassis rear panel (2 of the 4 rear DB9s, for CAN bus and SWD/JTAG debug).

This directory groups all hardware design artifacts for this subsystem:

```
companion_board/
├── pcb/                       → KiCad PCB project (Arduino Due shield, current rev: v4)
│   ├── README.md              → workflow (open KiCad, place footprints, route, export to fab)
│   ├── pcb_backplane_v4.*     → KiCad 8 project + skeleton PCB (current)
│   ├── pcb_backplane_v3.*     → previous-rev skeleton (101.6 × 53.34 mm, kept for history)
│   └── .gitignore             → excludes KiCad per-user local files
│
└── panel_sketch/              → Panel design references (sketches + photos + 1:1 placement guides)
    ├── front_panel.svg               → front panel drilling/labeling template
    ├── rear_panel.svg                → rear panel layout target (see DEVICE_PINOUT.md §3)
    ├── pcb_backplane_top.svg         → backplane v4 top layer 1:1 placement guide
    ├── pcb_backplane_bottom.svg      → backplane v4 bottom layer 1:1 placement guide (X-ray view)
    ├── fronte.jpg                    → front panel photo (reference)
    └── retro.jpg                     → rear panel photo (reference)
```

## Quick links

- **PCB CAD workflow** → [`pcb/README.md`](pcb/README.md)
- **Full pinout (v4)** → [`../docs/technical/PCB_BACKPLANE_PINOUT.md`](../docs/technical/PCB_BACKPLANE_PINOUT.md)
- **Device pinout (front + rear panel)** → [`../docs/technical/DEVICE_PINOUT.md`](../docs/technical/DEVICE_PINOUT.md)

## What this board does (v4)

**Mostly-passive Arduino Due shield** (120 × 100 mm, 4-layer, fab-service produced) that:

1. **Plugs directly onto the Arduino Due** via 8 male pin headers on its bottom side: 6 standard Mega R3 mating headers (POWER, ANALOG, COMM, DIGITAL, 2×18 D22-D53, DAC/CAN) plus 2 Due-extension headers (`H_UART` for D14-D21, `H_AEXT` for A8-A11 + D70/D71). No wires from the Due to the backplane.
2. **Fans out all firmware-published Due signals** to 10 D-Sub IDC headers on its top side: 8 connect to the front-panel D-Subs via short ribbon cables, 2 connect to the rear-panel CAN and JTAG DB9s. Total: 140 pins distributed (2×DB25 + 3×DB15 + 5×DB9). The bottom-left front DB25 (DB25-3) is intentionally NOT routed here, it stays as the host's parallel-port breakout.
3. **Hosts the CAN transceiver subcircuit** in the east margin: MCP2562FD DIP-8 socket, 120 Ω termination resistor, DPST switch to enable/disable termination, decoupling. The Due CAN0 controller (PA0/PA1) talks differential CAN_H/CAN_L to the rear DB9 #4 with no separate daughter-PCB.
4. **Routes SWD/JTAG** from the Due-J1 1.27 mm SMD header to the rear DB9 #5 via a small PCB cutout above J1 plus a custom pigtail (1.27 mm IDC ↔ 2.54 mm IDC) plus an internal bridge header (J11) plus copper traces to J10.
5. **4-layer stackup** (F.Cu signals, In1.Cu continuous GND plane, In2.Cu split PWR plane for +3.3 V and +5 V, B.Cu signals): chosen for cleaner ADC noise floor and a proper return path for the CAN differential pair.

All signals on the front-panel D-Subs are **native 3.3 V CMOS** at the Due output level. For 5 V TTL or 24 V industrial loads, build an application-specific breakout that plugs into the appropriate D-Sub. The rear DB9 CAN signals are already conditioned (true differential CAN bus). The rear DB9 JTAG is direct from the SAM3X (3.3 V, contained inside the chassis).

## Design history

- **v1** (abandoned): active design with onboard level shifters, opto-isolation, 12 V→5 V buck + 12 V→24 V boost. Too complex, too many ICs to hand-solder.
- **v2** (abandoned): passive 140 × 80 mm standalone PCB, single-layer, connected to Due via a 50-wire harness. Worked but cable management was messy.
- **v3** (skeleton, never fabricated): passive Arduino Due shield, 101.6 × 53.34 mm Due footprint exact, double-sided home-fab with through-hole rivets for vias. Skeleton committed but the design was incomplete (no CAN, no JTAG, no UART, only 8 of 12 ADCs exposed).
- **v4** (current, design sketch): 120 × 100 mm fab-service shield, 4-layer with continuous GND plane and split PWR plane. 8 bottom-side mating headers cover all firmware-published Due pins. On-board MCP2562FD CAN transceiver with switchable termination. JTAG access through a PCB cutout + pigtail. 10 D-Sub IDC headers on top (2 DB25 + 3 DB15 + 5 DB9 = 140 pins), 8 to the front and 2 to the rear. See `pcb/` and `../docs/technical/PCB_BACKPLANE_PINOUT.md`.
