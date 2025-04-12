# phycmd — Python bindings

Hard-real-time Python API for the PhyCommander Rust control stack.
Built with [pyo3](https://pyo3.rs) and packaged by
[maturin](https://www.maturin.rs).

## Install from source

```bash
# Inside the `physerver/crates/phycmd-py/` directory:
python -m venv .venv
source .venv/bin/activate
pip install maturin
maturin develop --release --features "pyo3/extension-module"
```

`maturin develop` compiles the Rust extension and installs the
resulting `phycmd` module into the active virtualenv, so you can
immediately `import phycmd` and use it.

To build a wheel for distribution instead of a development install:

```bash
maturin build --release
# produces target/wheels/phycmd-<version>-*.whl
pip install target/wheels/phycmd-*.whl
```

## Quick start

```python
import phycmd
import time

# Open a mock loopback PhyCommander — no USB hardware required.
phy = phycmd.PhyCommander.open_mock(rate_hz=10_000, mock_latency_us=50)

# Default mode is BlockUntilSent — two writes to the same field
# serialise at the tick rate.
phy.set_dac0(1234)
phy.set_dac0(5678)   # blocks until 1234 has hit the wire

# Override the mode per call:
phy.set_dac0(9012, mode=phycmd.WriteMode.Coalesce)

# Subscribe to status frames via a callback.
def on_frame(frame):
    print(f"seq={frame['cmd_seq']} adc0={frame['status']['adc'][0]} "
          f"jitter={frame['jitter_us']}us")

phy.subscribe(on_frame)

time.sleep(1.0)
print(phy.stats())
phy.stop()
```

## Concurrency model

The RT scheduler runs in its own dedicated Rust thread (created by
the underlying `phycmd_rust::PhyCommander`). All Python write methods
release the GIL around the underlying Rust call, so a
`BlockUntilSent` wait does not block other Python threads.

`subscribe()` spawns a dispatcher thread per callback. The
dispatcher consumes from a `broadcast::Receiver<StatusFrame>` in
Rust, acquires the GIL with `Python::with_gil`, and invokes the
Python callable with a dict view of the frame. Slow callbacks delay
only their own dispatcher; the RT loop keeps ticking regardless.

Subscribers are torn down automatically when the `PhyCommander`
instance is garbage-collected or `stop()` is called.
