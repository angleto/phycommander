# `applications/` — Host-side application software

This folder is the top-level home for **host-side control software** for
every application (instrument, experiment, use case) built on
phycommander. Each application lives in its own subfolder, with a
consistent internal structure, so the codebase scales cleanly as more
instruments are added.

Each subfolder here has a one-to-one correspondence with a matching
subfolder under [`../docs/applications/`](../docs/applications/):
`applications/X/` is the code for what
`docs/applications/X/` documents.

## Currently available applications

| Application | Folder | Language | Depends on |
|---|---|---|---|
| **Multi-function analyzer** | [`analyzer/`](analyzer/) | Python 3.11+ (numpy, scipy, matplotlib, reportlab) | phycommander firmware v1.1, physerver (Rust) via IPC |

## Structure conventions

Every application subfolder follows a standard Python project layout:

```
<application-name>/
├── README.md                   entry point, install, usage
├── pyproject.toml              Python project metadata + dependencies
├── <application-name>/         the Python package itself
│   ├── __init__.py
│   ├── main.py                 CLI entry point
│   ├── config.py               global configuration
│   ├── lockin.py               shared lock-in engine (if applicable)
│   ├── database.py             persistence layer
│   ├── calibration.py          calibration manager
│   ├── modes/                  measurement mode controllers
│   ├── assays/                 chemistry / physics protocols
│   ├── hardware/               hardware abstraction (module enum, drivers)
│   └── ui/                     text or graphical UI
├── tests/                      pytest test suite
├── scripts/                    standalone executable scripts
└── config/                     TOML configuration files
```

When adding a new application:

1. Create `applications/<new-app-name>/` with the structure above.
2. Create the matching documentation folder at
   `docs/applications/<new-app-name>/`.
3. Add an entry to the table above.
4. Add the dependency on `physerver` (or whatever transport layer the
   new application needs) in `pyproject.toml`.

## Running an application

See the individual `README.md` in each application folder for
install and run instructions. Typical invocation:

```bash
cd applications/<app-name>
pip install -e .              # one-time install
python -m <app-name>.main     # run
```

## Related

- **Documentation**: [`../docs/applications/`](../docs/applications/) — mirrors this tree one-for-one.
- **phycommander core server**: [`../physerver/`](../physerver/) — Rust server that the applications connect to.
- **phycommander firmware**: [`../ATSAM3X8E_FW/`](../ATSAM3X8E_FW/) — embedded side.
