// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Angelo Leto <angelo@leto.blue>

//! Fuzz `decode_status` with arbitrary 64-byte-ish frames.
//!
//! The function is called on every iso IN packet that makes it past
//! the USB layer, so any panic/UB here would crash the server when
//! a malformed device or a wire flip corrupts a frame. The goal is
//! "garbage in -> clean Err, never panic".
//!
//! Run with:
//!   cargo +nightly fuzz run decode_status

#![no_main]

use libfuzzer_sys::fuzz_target;
use phycmd_core::protocol::decode_status;

fuzz_target!(|data: &[u8]| {
    // Frames shorter than 64 bytes should be rejected cleanly, not
    // sliced OOB. Frames longer than 64 bytes should also be
    // handled — decode_status takes a `&[u8]` and only inspects the
    // first 64 bytes anyway.
    let _ = decode_status(data);
});
