# Companion Board

Hardware design for the phycommander **companion board** — the physical layer that adapts the Arduino Due microcontroller to the chassis front panel (9 D-Sub connectors) and routes signals between them.

This directory groups all hardware design artifacts for this subsystem:

```
companion_board/
├── pcb/                 → KiCad PCB project (Arduino Due shield v3)
│   ├── README.md        → workflow (open KiCad, place footprints, route, export)
│   ├── *.kicad_pro      → KiCad 8 project
│   ├── *.kicad_pcb      → Board layout (outline + mounting holes + placement guides)
│   └── .gitignore       → excludes KiCad per-user local files
│
└── panel_sketch/        → Panel design reference (sketches + photos + 1:1 masters)
    ├── front_panel.svg               → front panel drilling/labeling template
    ├── rear_panel.svg                → rear panel desiderata (design target, see DEVICE_PINOUT.md §3)
    ├── pcb_backplane_top.svg         → PCB top layer master, 1:1 scale, print-ready for UV exposure
    ├── pcb_backplane_bottom.svg      → PCB bottom layer master, 1:1 scale, MIRROR before print
    ├── fronte.jpg                    → front panel photo (reference)
    └── retro.jpg                     → rear panel photo (reference)
```

## Quick links

- **PCB CAD workflow** → [`pcb/README.md`](pcb/README.md)
- **Full pinout** → [`../docs/technical/PCB_BACKPLANE_PINOUT.md`](../docs/technical/PCB_BACKPLANE_PINOUT.md)
- **Device pinout (front + rear panel)** → [`../docs/technical/DEVICE_PINOUT.md`](../docs/technical/DEVICE_PINOUT.md)

## What this board does

Passive **Arduino Due shield** (101.60 × 53.34 mm, double-sided home-fab) that:

1. **Plugs directly onto the Arduino Due** via male pin headers on its bottom side (POWER, ANALOG, COMM, DIGITAL, 2×18 D22–D53, DAC/CAN — all standard Arduino Mega R3 shield-compatible positions)
2. **Fans out all 50+ I/O signals** from the Due to 9 D-Sub male pin headers on its top side, via copper traces on 2 layers connected by through-hole rivets (no plated vias — designed for home-fab with UV bromograph + photosensitized FR-4)
3. **Each of the 9 D-Sub headers** drives one front-panel D-Sub female connector via IDC ribbon cable — 3×DB25 + 3×DB15 + 3×DB9 = 147 pins total, all populated

All signals are **native 3.3 V CMOS** — no level shifting on this board. For 5 V TTL or 24 V industrial loads, build an application-specific breakout that plugs into the appropriate front-panel D-Sub.

## Design history

- **v1** (abandoned): active design with onboard level shifters, opto-isolation, 12 V→5 V buck + 12 V→24 V boost. Too complex, too many ICs to hand-solder.
- **v2** (abandoned): passive 140×80 mm standalone PCB, single-layer, connected to Due via 50-wire harness. Worked but cable management was messy.
- **v3** (current): passive Arduino Due shield, double-sided 101.60×53.34 mm, plugs directly onto Due. No wire harness. See `pcb/`.
