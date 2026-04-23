// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Angelo Leto <angelo@leto.blue>

//! Fuzz `decode_command` with arbitrary bytes. Parallel to
//! decode_status: the firmware calls this on every iso OUT packet,
//! so any panic here would be a remote DoS via malformed commands.
//!
//! Run with:
//!   cargo +nightly fuzz run decode_command

#![no_main]

use libfuzzer_sys::fuzz_target;
use phycmd_core::protocol::decode_command;

fuzz_target!(|data: &[u8]| {
    let _ = decode_command(data);
});
