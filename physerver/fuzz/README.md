# phycmd fuzz harness

Coverage-guided fuzz targets for the wire-facing codecs. Runs under
`cargo-fuzz` (libfuzzer) on nightly Rust.

## Setup

```bash
cargo install cargo-fuzz       # once, per host
rustup toolchain install nightly
```

## Run

```bash
# from physerver/ (not physerver/fuzz/):
cargo +nightly fuzz run decode_status           # iso IN path
cargo +nightly fuzz run decode_command          # iso OUT / bulk path
cargo +nightly fuzz run wave_types_roundtrip    # REST JSON path
```

Stop any run with Ctrl-C; corpus and crashes are saved under
`fuzz/corpus/<target>/` and `fuzz/artifacts/<target>/`.

## What to look for

- **Panics**: any crash in these targets is a server-reachable DoS.
  Fix the decode path (validate length, header, CRC before touching
  fields) before landing unrelated changes.
- **Asymmetric serde round-trips** in `wave_types_roundtrip`: the
  target asserts that `parse -> serialize -> parse` yields an
  equivalent value. A `panic` there means the two sides of the
  `Serialize` / `Deserialize` impl disagree.

## Why these targets

| target | wire role | attack surface |
|---|---|---|
| `decode_status` | device -> host iso IN | Due sends malformed frame, host panics |
| `decode_command` | host -> device iso OUT | any LAN client posts rubbish to `/api/command` |
| `wave_types_roundtrip` | REST `/api/waveform/:channel` | same, plus serde reflexivity |

The scheduler / transport / web layers aren't fuzzed here because
they pull in async runtimes and hardware handles that don't sandbox
well under libfuzzer. If scheduler state corruption ever becomes a
concern, use `proptest` in an integration test instead.

## Excluding from the workspace

`fuzz/Cargo.toml` carries its own `[workspace]` block so that
cargo-fuzz's nightly-only flags (`-Zbuild-std`, sanitizer) don't
poison the parent workspace when you build it with stable toolchain.
If you add a new fuzz target, declare its `[[bin]]` entry here and
keep it behind the local workspace.
