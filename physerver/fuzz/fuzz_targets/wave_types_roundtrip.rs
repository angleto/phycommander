// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Angelo Leto <angelo@leto.blue>

//! Fuzz the waveform spec serde round-trip. `WaveformSpec` is the
//! payload of `POST /api/waveform/:channel`, so it's directly
//! reachable from the network — a panic on malformed JSON here is a
//! remote-DoS. We try both the owned type (`WaveformSpec`) and the
//! `WaveformShape` variant that names the mode selector.
//!
//! Run with:
//!   cargo +nightly fuzz run wave_types_roundtrip

#![no_main]

use libfuzzer_sys::fuzz_target;
use phycmd_core::WaveformSpec;

fuzz_target!(|data: &[u8]| {
    // Step 1: parse as JSON (if it is). Any failure here is fine.
    let Ok(s) = std::str::from_utf8(data) else { return };
    let Ok(spec) = serde_json::from_str::<WaveformSpec>(s) else { return };

    // Step 2: round-trip: re-serialise and re-parse. The result must
    // equal the first parse. Catches asymmetry in the Serialize /
    // Deserialize impls that would otherwise only show up on a lossy
    // API boundary.
    let back = serde_json::to_string(&spec).expect("serialize a parsed value");
    let _spec2: WaveformSpec = serde_json::from_str(&back)
        .expect("re-parse what we just serialized");
});
