#!/usr/bin/env python3
"""Honest Datara-vs-Rust benchmark harness.

Why this exists
---------------
The previous harness (``time.sh``) timed whole processes from a Git Bash shell.
That measured MSYS process-spawn overhead, not the workload: an empty ``main``
reported ~3.5 s and a 2-billion-iteration loop reported ~3.6 s. Its own control
(a program sleeping 300 ms, measured at 3.9 s) proved the harness was invalid,
yet the numbers were recorded anyway.

This harness measures the **kernel only**. Each benchmark prints two lines:

    line 1: the checksum (proves the loop actually ran and was not folded away)
    line 2: the wall time of the kernel in milliseconds, measured *inside* the
            process (Datara via ``now_ms()``, Rust via ``Instant``)

Process startup is reported separately so it can never be mistaken for compute.

Methodology
-----------
* Identical algorithms in both languages, written side by side in ``workloads/``.
* Trip counts are derived from the wall clock at run time, so no compiler can
  constant-fold the loop, and both languages see the same N.
* Outputs are checksummed; a mismatch fails the run instead of being averaged in.
* Every measurement is repeated; min / median / mean are all reported.
* Raw data can be dumped as JSON for publication.

Usage
-----
    python harness.py                       # all workloads, 7 runs each
    python harness.py --runs 15
    python harness.py --workloads int_loop float_loop
    python harness.py --json results.json
"""

from __future__ import annotations

import argparse
import json
import math
import os
import pathlib
import shutil
import statistics
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
WORKLOADS = HERE / "workloads"
BIN = HERE / "bin"

# Datara's now_ms() is backed by QueryPerformanceCounter but truncated to whole
# milliseconds, so a kernel shorter than ~200 ms carries >0.5% quantization
# error. Anything faster is flagged rather than silently reported.
MIN_RELIABLE_KERNEL_MS = 200.0

FLOAT_TOLERANCE = 1e-9


def project_root() -> pathlib.Path:
    return HERE.parent


def forgen_binary() -> pathlib.Path:
    exe = project_root() / "target" / "release" / "forgen.exe"
    if not exe.exists():
        sys.exit(f"forgen release binary not found: {exe}\nRun: cargo build --release")
    return exe


def rustc() -> str:
    path = shutil.which("rustc")
    if path is None:
        home = pathlib.Path.home()
        candidate = home / ".cargo" / "bin" / "rustc.exe"
        if candidate.exists():
            return str(candidate)
        sys.exit("rustc not found on PATH or in ~/.cargo/bin")
    return path


def run(cmd: list[str], cwd: pathlib.Path | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=900)


def build_datara(name: str) -> pathlib.Path:
    """Compile workloads/<name>.dtr. Forgen writes the exe next to the source."""
    src = WORKLOADS / f"{name}.dtr"
    exe = WORKLOADS / f"{name}.exe"
    if not src.exists():
        sys.exit(f"missing workload source: {src}")

    res = run([str(forgen_binary()), "build", str(src)], cwd=HERE)
    if res.returncode != 0 or not exe.exists():
        sys.exit(f"Datara build failed for {name}:\n{res.stdout}\n{res.stderr}")
    return exe


def build_rust(name: str) -> pathlib.Path:
    """Compile workloads/<name>.rs with optimizations into bin/."""
    BIN.mkdir(exist_ok=True)
    src = WORKLOADS / f"{name}.rs"
    exe = BIN / f"{name}.exe"
    if not src.exists():
        sys.exit(f"missing workload source: {src}")

    res = run([rustc(), "-O", "-C", "debuginfo=0", "-o", str(exe), str(src)], cwd=HERE)
    if res.returncode != 0 or not exe.exists():
        sys.exit(f"Rust build failed for {name}:\n{res.stdout}\n{res.stderr}")
    return exe


def parse_output(text: str) -> tuple[str, float]:
    """Return (checksum, kernel_ms) from a benchmark's stdout."""
    lines = [ln.strip() for ln in text.strip().splitlines() if ln.strip()]
    if len(lines) < 2:
        raise ValueError(f"expected 2 output lines (checksum, kernel ms), got {lines!r}")
    return lines[-2], float(lines[-1])


def measure(exe: pathlib.Path, runs: int) -> dict:
    """Run a benchmark `runs` times, reporting kernel stats and startup cost."""
    kernels: list[float] = []
    startups: list[float] = []
    checksums: list[str] = []

    for i in range(runs + 1):  # first run is warm-up and is discarded
        wall_start = time.perf_counter()
        res = run([str(exe)])
        wall = (time.perf_counter() - wall_start) * 1000.0
        if res.returncode != 0:
            raise RuntimeError(f"{exe.name} exited {res.returncode}: {res.stderr.strip()}")

        checksum, kernel = parse_output(res.stdout)
        if i == 0:
            continue  # warm-up: page in the binary, let the CPU settle
        checksums.append(checksum)
        kernels.append(kernel)
        startups.append(wall - kernel)

    if len(set(checksums)) != 1:
        raise RuntimeError(f"{exe.name} produced unstable checksums: {set(checksums)}")

    return {
        "checksum": checksums[0],
        "kernel_min": min(kernels),
        "kernel_median": statistics.median(kernels),
        "kernel_mean": statistics.fmean(kernels),
        "kernel_samples": kernels,
        "startup_median": statistics.median(startups),
    }


def checksums_match(a: str, b: str) -> bool:
    try:
        fa, fb = float(a), float(b)
    except ValueError:
        return a == b
    if math.isnan(fa) and math.isnan(fb):
        return True
    # Float kernels may legitimately differ in the last bits if one compiler
    # reassociates; require relative agreement instead of exact equality.
    if fa == fb:
        return True
    denom = max(abs(fa), abs(fb))
    return denom == 0.0 or abs(fa - fb) / denom <= FLOAT_TOLERANCE


def benchmark(name: str, runs: int) -> dict:
    forgen_compile_ms = None
    start = time.perf_counter()
    datara_exe = build_datara(name)
    forgen_compile_ms = (time.perf_counter() - start) * 1000.0

    rustc_compile_ms = None
    start = time.perf_counter()
    rust_exe = build_rust(name)
    rustc_compile_ms = (time.perf_counter() - start) * 1000.0

    datara = measure(datara_exe, runs)
    rust = measure(rust_exe, runs)

    ok = checksums_match(datara["checksum"], rust["checksum"])
    ratio = datara["kernel_median"] / rust["kernel_median"] if rust["kernel_median"] else float("nan")

    return {
        "workload": name,
        "datara": datara,
        "rust": rust,
        "correct": ok,
        "ratio_datara_vs_rust": ratio,
        "forgen_compile_ms": forgen_compile_ms,
        "rustc_compile_ms": rustc_compile_ms,
        "datara_binary_bytes": datara_exe.stat().st_size,
        "rust_binary_bytes": rust_exe.stat().st_size,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--runs", type=int, default=7, help="timed repetitions per benchmark (default 7)")
    ap.add_argument("--workloads", nargs="*", default=None, help="subset of workload names")
    ap.add_argument("--json", type=pathlib.Path, default=None, help="write raw measurements here")
    args = ap.parse_args()

    names = args.workloads or sorted(p.stem for p in WORKLOADS.glob("*.dtr"))
    if not names:
        sys.exit("no workloads found in " + str(WORKLOADS))

    print(f"Datara vs Rust - kernel-only timing, {args.runs} timed runs per benchmark")
    print(f"Compiler: forgen release  |  Reference: rustc -O")
    print()

    results = []
    failures = 0

    for name in names:
        try:
            r = benchmark(name, args.runs)
        except (RuntimeError, ValueError) as e:
            print(f"{name:<14} ERROR: {e}")
            failures += 1
            continue
        results.append(r)

        d, ru = r["datara"], r["rust"]
        flag = "OK " if r["correct"] else "BAD"

        # A kernel near zero on one side only means that compiler deleted the
        # loop (LLVM reduces `sum += i` over 0..n to a closed form). The ratio
        # would be meaningless, so say so instead of printing nan.
        eliminated = None
        if ru["kernel_median"] < 1.0 < d["kernel_median"]:
            eliminated = "rustc"
        elif d["kernel_median"] < 1.0 < ru["kernel_median"]:
            eliminated = "forgen"

        print(f"{name:<14} {flag} checksum verified")
        print(f"  Datara  kernel min {d['kernel_min']:8.1f} ms   median {d['kernel_median']:8.1f} ms   mean {d['kernel_mean']:8.1f} ms")
        print(f"  Rust    kernel min {ru['kernel_min']:8.1f} ms   median {ru['kernel_median']:8.1f} ms   mean {ru['kernel_mean']:8.1f} ms")
        if eliminated:
            print(f"  ratio   NOT COMPARABLE: {eliminated} eliminated this kernel entirely "
                  f"(loop-idiom recognition); Forgen cannot, so no ratio is meaningful")
        else:
            print(f"  ratio   Datara/Rust = {r['ratio_datara_vs_rust']:.2f}x   (lower is better for Datara)")
        print(f"  startup Datara {d['startup_median']:.1f} ms   Rust {ru['startup_median']:.1f} ms  (excluded from kernel)")
        print(f"  compile forgen {r['forgen_compile_ms']:.0f} ms   rustc {r['rustc_compile_ms']:.0f} ms")
        print(f"  binary  Datara {r['datara_binary_bytes']} B   Rust {r['rust_binary_bytes']} B")
        if not eliminated and min(d["kernel_median"], ru["kernel_median"]) < MIN_RELIABLE_KERNEL_MS:
            print(f"  note    kernel <{MIN_RELIABLE_KERNEL_MS:.0f}ms: Datara's 1ms timer granularity is significant")
        print()

    if results:
        verified = sum(1 for r in results if r["correct"])
        print(f"Summary: {verified}/{len(results)} workloads produced matching checksums")
        ratios = [
            r["ratio_datara_vs_rust"]
            for r in results
            if r["correct"]
            and min(r["datara"]["kernel_median"], r["rust"]["kernel_median"]) >= 1.0
        ]
        if ratios:
            print(f"         Datara/Rust kernel ratio: median {statistics.median(ratios):.2f}x, "
                  f"range {min(ratios):.2f}x-{max(ratios):.2f}x")

    if args.json:
        payload = []
        for r in results:
            payload.append({
                k: (v if k not in ("datara", "rust") else {
                    kk: vv for kk, vv in v.items() if kk != "kernel_samples"
                } | {"kernel_samples": v["kernel_samples"]})
                for k, v in r.items()
            })
        args.json.write_text(json.dumps(payload, indent=2), encoding="utf-8")
        print(f"\nRaw data written to {args.json}")

    return 1 if failures or any(not r["correct"] for r in results) else 0


if __name__ == "__main__":
    sys.exit(main())
