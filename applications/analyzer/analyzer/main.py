"""CLI entry point for the analyzer application.

Usage:

    python -m analyzer.main                      # interactive TUI
    python -m analyzer.main --mode absorbance    # run one mode
    python -m analyzer.main --assay ethanol      # run one assay
    python -m analyzer.main --list               # list available modes/assays
    python -m analyzer.main --self-test          # run the electronic self-test
    python -m analyzer.main --calibrate ethanol  # run the calibration wizard
"""

from __future__ import annotations

import argparse
import logging
import sys
from pathlib import Path

from . import __version__
from .config import Config, load_config


def setup_logging(verbose: bool = False) -> None:
    level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(
        level=level,
        format="%(asctime)s [%(name)s] %(levelname)s: %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="analyzer",
        description=(
            "Host application for the phycommander multi-function "
            "analytical bench instrument."
        ),
    )
    parser.add_argument("--version", action="version", version=f"analyzer {__version__}")
    parser.add_argument(
        "--config",
        type=Path,
        help="Path to channels.toml configuration file.",
    )
    parser.add_argument(
        "--mode",
        choices=["absorbance", "kinetic", "nephelometry"],
        help="Run a specific measurement mode and exit.",
    )
    parser.add_argument(
        "--assay",
        choices=["ethanol", "bradford", "color_ebc", "haze_ebc", "folin"],
        help="Run a specific assay protocol and exit.",
    )
    parser.add_argument(
        "--calibrate",
        metavar="ASSAY",
        help="Run the calibration wizard for ASSAY with known standards.",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run the electronic self-test (DAC loopback, noise, interlock).",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="List available modes and assays and exit.",
    )
    parser.add_argument(
        "--sample-id",
        help="Sample identifier for the measurement (default: prompt).",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="Enable debug logging.",
    )
    return parser.parse_args(argv)


def list_capabilities(config: Config) -> None:
    print(f"Analyzer v{__version__}")
    print()
    print("Configured channels:")
    for ch in config.channels:
        flags = []
        if ch.safety_gated:
            flags.append("SAFETY-GATED")
        if not ch.enabled:
            flags.append("DISABLED")
        flag_str = f" [{', '.join(flags)}]" if flags else ""
        print(
            f"  {ch.name:<18}  λ={ch.wavelength_nm:>4.0f} nm   "
            f"f={ch.freq_hz:>6.1f} Hz   gate={ch.gate_kind}:{ch.gate_index}{flag_str}"
        )
    print()
    print("Available modes:")
    print("  absorbance      multi-channel Beer-Lambert absorbance")
    print("  kinetic         absorbance vs time (for enzyme assays)")
    print("  nephelometry    90° scattering (haze / turbidity)")
    print()
    print("Available assays:")
    print("  ethanol         ADH/NADH enzymatic, 340 nm")
    print("  bradford        protein, Coomassie G-250, 590 nm")
    print("  color_ebc       beer color at 430 nm")
    print("  haze_ebc        beer haze at 650 nm (90° scatter)")
    print("  folin           polyphenols via Folin-Ciocalteu, 740 nm")


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    setup_logging(args.verbose)
    log = logging.getLogger("analyzer.main")

    try:
        config = load_config(args.config)
    except FileNotFoundError as exc:
        log.error("%s", exc)
        return 1

    if args.list:
        list_capabilities(config)
        return 0

    if args.self_test:
        from scripts.self_test import run_self_test

        return run_self_test(config)

    if args.calibrate:
        log.info("Calibration wizard for assay %r not yet implemented.", args.calibrate)
        log.info("Implement calibrate_<assay>() in the corresponding assays/ module.")
        return 2

    if args.assay:
        return run_assay(args.assay, config, args.sample_id)

    if args.mode:
        return run_mode(args.mode, config, args.sample_id)

    # Default: launch interactive TUI
    log.info("Interactive TUI not yet implemented. Use --mode or --assay for now.")
    log.info("Run `analyzer --list` to see available options.")
    return 0


def run_mode(mode_name: str, config: Config, sample_id: str | None) -> int:
    """Dispatch to a mode controller."""
    log = logging.getLogger("analyzer.main")
    if sample_id is None:
        sample_id = input("Sample ID: ").strip() or "unnamed"
    log.info("Running mode %r on sample %r", mode_name, sample_id)

    if mode_name == "absorbance":
        from .modes.absorbance import AbsorbanceMode

        mode = AbsorbanceMode(config)
    elif mode_name == "kinetic":
        from .modes.kinetic import KineticMode

        mode = KineticMode(config)
    elif mode_name == "nephelometry":
        from .modes.nephelometry import NephelometryMode

        mode = NephelometryMode(config)
    else:
        log.error("Unknown mode: %s", mode_name)
        return 2

    mode.run(sample_id)
    return 0


def run_assay(assay_name: str, config: Config, sample_id: str | None) -> int:
    """Dispatch to an assay protocol."""
    log = logging.getLogger("analyzer.main")
    if sample_id is None:
        sample_id = input("Sample ID: ").strip() or "unnamed"
    log.info("Running assay %r on sample %r", assay_name, sample_id)

    assay_modules = {
        "ethanol": "analyzer.assays.ethanol",
        "bradford": "analyzer.assays.bradford",
        "color_ebc": "analyzer.assays.color_ebc",
        "haze_ebc": "analyzer.assays.haze_ebc",
        "folin": "analyzer.assays.folin",
    }

    if assay_name not in assay_modules:
        log.error("Unknown assay: %s", assay_name)
        return 2

    import importlib

    module = importlib.import_module(assay_modules[assay_name])
    assay_class = getattr(module, "Assay")
    assay = assay_class(config)
    result = assay.run(sample_id)
    print(result)
    return 0


if __name__ == "__main__":
    sys.exit(main())
