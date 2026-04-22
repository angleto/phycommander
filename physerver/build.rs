// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Angelo Leto <angelo@leto.blue>

//! Capture build-time metadata (git hash, build date, target triple)
//! as environment variables the `physerver` binary can read via
//! `env!()`. Exposed through the `/api/version` REST endpoint so
//! operators can confirm which commit is running on a deployed
//! host without reading systemd or the binary's timestamp.

use std::process::Command;

fn main() {
    // Short git hash + "-dirty" suffix if working tree has changes.
    // Falls back to "unknown" when building from an rsync-staged
    // tree without .git (e.g. remote deploys), or if git isn't on
    // the build host's PATH. Consumers of /api/version should treat
    // "unknown" as "built from a source tarball, no provenance"
    // rather than as a bug.
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short=10", "HEAD"])
        .output()
        .ok()
        .and_then(|o| if o.status.success() { String::from_utf8(o.stdout).ok() } else { None })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    let git = if dirty {
        format!("{git_hash}-dirty")
    } else {
        git_hash
    };

    // Build timestamp in ISO-8601 UTC. Using `date -u +...` keeps the
    // build.rs dependency-free; we're fine with ~1 s granularity.
    let build_date = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .and_then(|o| if o.status.success() { String::from_utf8(o.stdout).ok() } else { None })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // TARGET is only available to build.rs, not the crate itself.
    // Re-emit as PHYCMD_TARGET so the web handler can read it.
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rustc-env=PHYCMD_GIT_HASH={git}");
    println!("cargo:rustc-env=PHYCMD_BUILD_DATE={build_date}");
    println!("cargo:rustc-env=PHYCMD_TARGET={target}");

    // Re-run if HEAD moves or the index changes. This keeps the
    // hash fresh across commits without forcing a rebuild on every
    // unrelated edit.
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/index");
    println!("cargo:rerun-if-changed=build.rs");
}
