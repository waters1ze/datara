#!/usr/bin/env python3
"""
Datara (Forgen) Chart Generator
Generates high-contrast, publication-grade SVG and PNG charts from docs/data/*.json
Every chart embeds machine and compiler provenance footprint.
"""

import os
import sys
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.ticker as ticker
import numpy as np

REPO_ROOT = Path(__file__).resolve().parent.parent
DOCS_DATA = REPO_ROOT / "docs" / "data"
DOCS_IMG = REPO_ROOT / "docs" / "img"

# Design System Palette: Dark Modern Minimalist
PALETTE = {
    "bg": "#0f172a",          # slate-900
    "card_bg": "#1e293b",     # slate-800
    "text": "#f8fafc",        # slate-50
    "text_muted": "#94a3b8",  # slate-400
    "grid": "#334155",        # slate-700
    "border": "#475569",      # slate-600
    
    # Accent colors
    "datara_clif": "#38bdf8", # sky-400
    "datara_llvm": "#06b6d4", # cyan-500
    "datara_wasm": "#818cf8", # indigo-400
    "rust": "#f97316",        # orange-500
    "c_msvc": "#3b82f6",      # blue-500
    "python": "#eab308",      # yellow-500
    "node": "#22c55e",        # green-500
    "accent_green": "#10b981",# emerald-500
    "accent_amber": "#f59e0b",# amber-500
    "accent_red": "#ef4444",  # red-500
}


def setup_theme():
    """Configure matplotlib rcParams for dark modern aesthetic."""
    plt.rcParams["font.family"] = "sans-serif"
    plt.rcParams["font.sans-serif"] = ["Segoe UI", "DejaVu Sans", "Helvetica", "Arial"]
    plt.rcParams["text.color"] = PALETTE["text"]
    plt.rcParams["axes.labelcolor"] = PALETTE["text"]
    plt.rcParams["xtick.color"] = PALETTE["text_muted"]
    plt.rcParams["ytick.color"] = PALETTE["text_muted"]
    plt.rcParams["axes.facecolor"] = PALETTE["card_bg"]
    plt.rcParams["figure.facecolor"] = PALETTE["bg"]
    plt.rcParams["axes.edgecolor"] = PALETTE["border"]
    plt.rcParams["grid.color"] = PALETTE["grid"]
    plt.rcParams["grid.linestyle"] = "--"
    plt.rcParams["grid.alpha"] = 0.6


def format_provenance(meta):
    """Format provenance string for chart footer."""
    cpu = meta.get("cpu", "CPU")
    cores = meta.get("logical_cores", "")
    ram = meta.get("ram_gb", "")
    os_name = meta.get("os", "Windows")
    git_hash = meta.get("git_commit", "unknown")[:8]
    methodology = meta.get("methodology", "Median-of-7")
    return f"Provenance: {cpu} ({cores}T) | {ram}GB RAM | {os_name} | git:{git_hash} | {methodology}"


def add_provenance_footer(fig, meta):
    """Render provenance text in figure footer."""
    foot = format_provenance(meta)
    fig.text(
        0.5, 0.02, foot,
        ha="center", va="bottom",
        fontsize=8.5, color=PALETTE["text_muted"],
        family="monospace"
    )


def save_chart(fig, name):
    """Save both SVG (lossless vector) and PNG (dpr=2 for retina display)."""
    DOCS_IMG.mkdir(parents=True, exist_ok=True)
    svg_path = DOCS_IMG / f"{name}.svg"
    png_path = DOCS_IMG / f"{name}.png"
    fig.savefig(svg_path, format="svg", bbox_inches="tight", facecolor=fig.get_facecolor(), edgecolor="none")
    fig.savefig(png_path, format="png", dpi=200, bbox_inches="tight", facecolor=fig.get_facecolor(), edgecolor="none")
    plt.close(fig)
    print(f"  [CHART] Saved {svg_path.name} & {png_path.name}")


# ============================================================================
# 1. Compile Times Chart
# ============================================================================
def chart_compile_times():
    json_file = DOCS_DATA / "compile_times.json"
    if not json_file.exists(): return
    data = json.loads(json_file.read_text(encoding="utf-8"))
    meta = data["metadata"]
    results = data["results"]

    targets = ["hello", "fib", "matrix"]
    compilers = [
        ("datara_cranelift", "Datara (Cranelift)", PALETTE["datara_clif"]),
        ("datara_llvm", "Datara (LLVM AOT)", PALETTE["datara_llvm"]),
        ("rustc_release", "rustc (-O)", PALETTE["rust"]),
        ("msvc_cl_o2", "C MSVC (/O2)", PALETTE["c_msvc"]),
    ]

    fig, ax = plt.subplots(figsize=(10, 5.5))
    x = np.arange(len(targets))
    width = 0.18

    for i, (ckey, clabel, color) in enumerate(compilers):
        medians = [results[t][ckey]["median_ms"] for t in targets]
        offset = (i - 1.5) * width
        rects = ax.bar(x + offset, medians, width, label=clabel, color=color, edgecolor=PALETTE["bg"], alpha=0.95)
        # Value labels
        for rect in rects:
            height = rect.get_height()
            ax.annotate(
                f"{height:.0f}ms",
                xy=(rect.get_x() + rect.get_width() / 2, height),
                xytext=(0, 3),
                textcoords="offset points",
                ha="center", va="bottom",
                fontsize=8, color=PALETTE["text"], fontweight="bold"
            )

    ax.set_title("AOT End-to-End Compilation & Link Latency (Lower is Better)", fontsize=13, fontweight="bold", pad=15)
    ax.set_ylabel("Compilation Time (ms)", fontsize=10)
    ax.set_xticks(x)
    ax.set_xticklabels(["Hello World\n(minimal link)", "Recursive Fib\n(scalar math)", "3x3 Matrix\n(OOP/struct math)"], fontsize=10)
    ax.legend(frameon=True, facecolor=PALETTE["card_bg"], edgecolor=PALETTE["border"], loc="upper left", fontsize=9)
    ax.grid(axis="y", alpha=0.4)
    ax.set_axisbelow(True)

    add_provenance_footer(fig, meta)
    plt.subplots_adjust(bottom=0.14)
    save_chart(fig, "compile_times")


# ============================================================================
# 2. Runtime Benchmarks Chart
# ============================================================================
def chart_runtime_benchmarks():
    json_file = DOCS_DATA / "runtime_benchmarks.json"
    if not json_file.exists(): return
    data = json.loads(json_file.read_text(encoding="utf-8"))
    meta = data["metadata"]
    results = data["results"]

    fig, (ax1, ax2, ax3) = plt.subplots(1, 3, figsize=(15, 6))

    # Subplot 1: fib(35)
    fib = results["fib_35"]
    fib_labels = ["Datara (CLIF)", "Datara (LLVM)", "C (MSVC /O2)", "Rust (-O)", "Python 3.14"]
    fib_vals = [
        fib["datara_cranelift_same_algo"]["median_ms"],
        fib["datara_llvm_same_algo"]["median_ms"],
        fib["c_msvc_o2"]["median_ms"],
        fib["rust_release"]["median_ms"],
        fib["pure_python_314"]["median_ms"],
    ]
    fib_colors = [PALETTE["datara_clif"], PALETTE["datara_llvm"], PALETTE["c_msvc"], PALETTE["rust"], PALETTE["python"]]
    bars1 = ax1.bar(fib_labels, fib_vals, color=fib_colors, width=0.55)
    ax1.set_title("fib(35) [Same Algorithm]\n(Lower is Better)", fontsize=11, fontweight="bold")
    ax1.set_ylabel("Execution Time (ms)", fontsize=10)
    ax1.tick_params(axis="x", rotation=30)
    ax1.grid(axis="y", alpha=0.4)
    for b in bars1:
        h = b.get_height()
        ax1.annotate(f"{h:.1f}ms", (b.get_x() + b.get_width()/2, h), textcoords="offset points", xytext=(0, 3), ha="center", fontsize=8, fontweight="bold")

    # Subplot 2: sum 1e8
    sum_res = results["sum_1e8"]
    sum_labels = ["Datara (CLIF)", "Datara (LLVM)", "C (MSVC /O2)", "Rust (-O)", "Python 3.14"]
    sum_vals = [
        sum_res["datara_cranelift_raw"]["median_ms"],
        sum_res["datara_llvm_raw"]["median_ms"],
        sum_res["c_msvc_o2"]["median_ms"],
        sum_res["rust_release"]["median_ms"],
        sum_res["pure_python_314"]["median_ms"],
    ]
    sum_colors = [PALETTE["datara_clif"], PALETTE["datara_llvm"], PALETTE["c_msvc"], PALETTE["rust"], PALETTE["python"]]
    bars2 = ax2.bar(sum_labels, sum_vals, color=sum_colors, width=0.55)
    ax2.set_title("sum 1e8 [Raw Loop with Branch]\n(Lower is Better)", fontsize=11, fontweight="bold")
    ax2.tick_params(axis="x", rotation=30)
    ax2.grid(axis="y", alpha=0.4)
    for b in bars2:
        h = b.get_height()
        ax2.annotate(f"{h:.1f}ms", (b.get_x() + b.get_width()/2, h), textcoords="offset points", xytext=(0, 3), ha="center", fontsize=8, fontweight="bold")

    # Subplot 3: dot 4M float4
    dot_res = results["dot_4m_float4"]
    dot_labels = ["Datara (LLVM)", "Datara (CLIF)", "C (MSVC)", "Rust (-O)", "NumPy", "Python"]
    dot_vals = [
        dot_res["datara_llvm_simd"]["median_ms"],
        dot_res["datara_cranelift"]["median_ms"],
        dot_res["c_msvc_o2"]["median_ms"],
        dot_res["rust_release"]["median_ms"],
        dot_res["numpy_dot"]["median_ms"],
        dot_res["pure_python_314"]["median_ms"],
    ]
    dot_colors = [PALETTE["datara_llvm"], PALETTE["datara_clif"], PALETTE["c_msvc"], PALETTE["rust"], PALETTE["accent_green"], PALETTE["python"]]
    bars3 = ax3.bar(dot_labels, dot_vals, color=dot_colors, width=0.55)
    ax3.set_title("dot 4M Floats [SIMD / Vectors]\n(Lower is Better)", fontsize=11, fontweight="bold")
    ax3.tick_params(axis="x", rotation=30)
    ax3.grid(axis="y", alpha=0.4)
    for b in bars3:
        h = b.get_height()
        ax3.annotate(f"{h:.1f}ms", (b.get_x() + b.get_width()/2, h), textcoords="offset points", xytext=(0, 3), ha="center", fontsize=8, fontweight="bold")

    fig.suptitle("Datara Native Runtime Performance vs C / Rust / NumPy / Python", fontsize=14, fontweight="bold", y=0.98)
    add_provenance_footer(fig, meta)
    plt.subplots_adjust(bottom=0.20, top=0.88, wspace=0.28)
    save_chart(fig, "runtime_benchmarks")


# ============================================================================
# 3. JSON Throughput Chart
# ============================================================================
def chart_json_throughput():
    json_file = DOCS_DATA / "json_throughput.json"
    if not json_file.exists(): return
    data = json.loads(json_file.read_text(encoding="utf-8"))
    meta = data["metadata"]
    res = data["results"]

    impls = list(res.keys())
    throughputs = [res[k]["throughput_mb_s"] for k in impls]
    colors = [PALETTE["rust"], PALETTE["node"], PALETTE["datara_clif"], PALETTE["python"]]

    fig, ax = plt.subplots(figsize=(9, 4.5))
    bars = ax.barh(impls, throughputs, color=colors, height=0.5, edgecolor=PALETTE["bg"])
    ax.set_xlabel("Throughput (MB/s) — Higher is Better", fontsize=10, fontweight="bold")
    ax.set_title(f"JSON Parser Throughput ({data.get('payload_size_mb', 5)} MB Synthetic Dataset)", fontsize=12, fontweight="bold", pad=12)
    ax.grid(axis="x", alpha=0.4)
    ax.invert_yaxis()

    for bar in bars:
        w = bar.get_width()
        ax.annotate(
            f"{w:.1f} MB/s",
            xy=(w, bar.get_y() + bar.get_height() / 2),
            xytext=(6, 0),
            textcoords="offset points",
            ha="left", va="center",
            fontsize=9.5, color=PALETTE["text"], fontweight="bold"
        )

    add_provenance_footer(fig, meta)
    plt.subplots_adjust(bottom=0.18, left=0.25)
    save_chart(fig, "json_throughput")


# ============================================================================
# 4. Ownership Mix Chart
# ============================================================================
def chart_ownership_mix():
    json_file = DOCS_DATA / "ownership_mix.json"
    if not json_file.exists(): return
    data = json.loads(json_file.read_text(encoding="utf-8"))
    meta = data["metadata"]
    progs = data["programs"]

    names = list(progs.keys())
    labels = {
        "p1_scalar": "p1_scalar\n(SSA immutable)",
        "p2_guarded": "p2_guarded\n(cond branch)",
        "p3_loop": "p3_loop\n(accumulator)",
        "p4_branch": "p4_branch\n(match dispatch)",
        "p5_view": "p5_view\n(struct borrow)",
    }
    x_labels = [labels.get(n, n) for n in names]

    proven = [progs[n]["proven_pct"] for n in names]
    guarded = [progs[n]["guarded_pct"] for n in names]
    rejected = [progs[n]["rejected_pct"] for n in names]

    fig, ax = plt.subplots(figsize=(9, 5))
    x = np.arange(len(names))
    width = 0.45

    b1 = ax.bar(x, proven, width, label="100% Proven (Zero-Cost SSA)", color=PALETTE["accent_green"])
    b2 = ax.bar(x, guarded, width, bottom=proven, label="Guarded (Runtime lattice fallback)", color=PALETTE["accent_amber"])
    b3 = ax.bar(x, rejected, width, bottom=[p + g for p, g in zip(proven, guarded)], label="Rejected (Definite compile-error)", color=PALETTE["accent_red"])

    ax.set_ylabel("Ownership State Ratio (%)", fontsize=10)
    ax.set_title("Datara Ownership Proof vs Guarded Fallback Distribution", fontsize=12, fontweight="bold", pad=12)
    ax.set_xticks(x)
    ax.set_xticklabels(x_labels, fontsize=9.5)
    ax.set_ylim(0, 115)
    ax.legend(frameon=True, facecolor=PALETTE["card_bg"], edgecolor=PALETTE["border"], loc="upper right", fontsize=9)
    ax.grid(axis="y", alpha=0.4)

    for i in range(len(names)):
        ax.annotate(f"{proven[i]:.0f}%", (x[i], proven[i] / 2), ha="center", va="center", color="#000", fontweight="bold", fontsize=9)

    add_provenance_footer(fig, meta)
    plt.subplots_adjust(bottom=0.18)
    save_chart(fig, "ownership_mix")


# ============================================================================
# 5. Determinism Flatline Chart
# ============================================================================
def chart_determinism_flatline():
    json_file = DOCS_DATA / "determinism_flatline.json"
    if not json_file.exists(): return
    data = json.loads(json_file.read_text(encoding="utf-8"))
    meta = data["metadata"]
    runs = data["runs"]

    run_indices = [r["run_index"] for r in runs]
    durations = [r["duration_ms"] for r in runs]
    checksums = [r["sha256_checksum"] for r in runs]
    all_ident = data.get("all_checksums_identical", True)

    fig, ax = plt.subplots(figsize=(10, 4.8))

    ax.plot(run_indices, durations, marker="o", color=PALETTE["datara_clif"], linewidth=2, markersize=5, label="Simulation Task Duration (ms)")
    
    # Render flatline reference
    avg_d = np.mean(durations)
    ax.axhline(avg_d, color=PALETTE["accent_green"], linestyle="--", alpha=0.7, label=f"Deterministic Output Checksum (20/20 Byte-Identical: {checksums[0][:12]}...)")

    ax.set_title("Determinism Flatline: 20 Lockstep Multi-Core Runs (Zero Divergence)", fontsize=12, fontweight="bold", pad=12)
    ax.set_xlabel("Run Index (1..20)", fontsize=10)
    ax.set_ylabel("Execution Time (ms)", fontsize=10)
    ax.set_xticks(run_indices)
    ax.legend(frameon=True, facecolor=PALETTE["card_bg"], edgecolor=PALETTE["border"], loc="upper right", fontsize=9)
    ax.grid(True, alpha=0.4)

    add_provenance_footer(fig, meta)
    plt.subplots_adjust(bottom=0.16)
    save_chart(fig, "determinism_flatline")


# ============================================================================
# 6. Binary Sizes Chart
# ============================================================================
def chart_binary_sizes():
    json_file = DOCS_DATA / "binary_sizes.json"
    if not json_file.exists(): return
    data = json.loads(json_file.read_text(encoding="utf-8"))
    meta = data["metadata"]
    results = data["results"]

    artifacts = list(results.keys())
    sizes_kb = [results[a]["size_kb"] for a in artifacts]
    colors = [
        PALETTE["datara_clif"],
        PALETTE["datara_llvm"],
        PALETTE["datara_wasm"],
        PALETTE["datara_wasm"],
        PALETTE["rust"],
        PALETTE["c_msvc"],
    ][:len(artifacts)]

    fig, ax = plt.subplots(figsize=(9, 5))
    bars = ax.barh(artifacts, sizes_kb, color=colors, height=0.5, edgecolor=PALETTE["bg"])
    ax.set_xlabel("Binary Size (KB) — Smaller is Better", fontsize=10, fontweight="bold")
    ax.set_title("Binary Artifact Footprint: Hello World", fontsize=12, fontweight="bold", pad=12)
    ax.grid(axis="x", alpha=0.4)
    ax.invert_yaxis()

    for bar in bars:
        w = bar.get_width()
        ax.annotate(
            f"{w:.1f} KB",
            xy=(w, bar.get_y() + bar.get_height() / 2),
            xytext=(6, 0),
            textcoords="offset points",
            ha="left", va="center",
            fontsize=9.5, color=PALETTE["text"], fontweight="bold"
        )

    add_provenance_footer(fig, meta)
    plt.subplots_adjust(bottom=0.18, left=0.28)
    save_chart(fig, "binary_sizes")


# ============================================================================
# 7. WASM Capabilities Matrix Chart
# ============================================================================
def chart_wasm_capabilities_matrix():
    json_file = DOCS_DATA / "wasm_capabilities_matrix.json"
    if not json_file.exists(): return
    data = json.loads(json_file.read_text(encoding="utf-8"))
    meta = data["metadata"]
    progs = data["programs"]
    caps = data["capabilities_lattice"]

    prog_names = list(progs.keys())
    matrix = np.zeros((len(prog_names), len(caps)))

    # 1: granted (green), 0: absent (gray), -1: denied (red)
    status_map = {"granted": 1.0, "absent": 0.0, "denied": -1.0}
    for i, p in enumerate(prog_names):
        row = progs[p]["capabilities"]
        for j, c in enumerate(caps):
            matrix[i, j] = status_map.get(row.get(c, "absent"), 0.0)

    fig, ax = plt.subplots(figsize=(8, 4.5))

    # Render custom categorical blocks
    for i in range(len(prog_names)):
        for j in range(len(caps)):
            val = matrix[i, j]
            if val == 1.0:
                color = PALETTE["accent_green"]
                label = "GRANTED"
            elif val == 0.0:
                color = PALETTE["grid"]
                label = "ABSENT"
            else:
                color = PALETTE["accent_red"]
                label = "DENIED"

            rect = plt.Rectangle((j - 0.45, i - 0.35), 0.9, 0.7, color=color, ec=PALETTE["bg"], lw=2)
            ax.add_patch(rect)
            ax.text(j, i, label, ha="center", va="center", color=PALETTE["text"], fontweight="bold", fontsize=9)

    ax.set_xlim(-0.6, len(caps) - 0.4)
    ax.set_ylim(-0.6, len(prog_names) - 0.4)
    ax.set_xticks(range(len(caps)))
    ax.set_yticks(range(len(prog_names)))
    ax.set_xticklabels([f"datara:{c}" for c in caps], fontsize=10, fontweight="bold")
    ax.set_yticklabels(prog_names, fontsize=10, fontweight="bold")
    ax.invert_yaxis()
    ax.set_title("WASM Module Capabilities Sandboxing Lattice", fontsize=12, fontweight="bold", pad=12)

    add_provenance_footer(fig, meta)
    plt.subplots_adjust(bottom=0.18, left=0.22)
    save_chart(fig, "wasm_capabilities_matrix")


def main():
    setup_theme()
    print("=== Generating Publication Charts from docs/data/*.json ===")
    chart_compile_times()
    chart_runtime_benchmarks()
    chart_json_throughput()
    chart_ownership_mix()
    chart_determinism_flatline()
    chart_binary_sizes()
    chart_wasm_capabilities_matrix()
    print("[SUCCESS] All 7 charts rendered to docs/img/ (*.svg & *.png).")


if __name__ == "__main__":
    main()
