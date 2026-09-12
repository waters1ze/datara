//! Cranelift JIT Optimization Tiers and Target ISA Configuration
//!
//! Provides two distinct tiers for JIT compilation:
//! - `FastCompile`: sub-millisecond compilation latency (< 3 us per function)
//!   using single-pass register allocation and opt_level=none.
//! - `MaxSpeed`: peak execution throughput (60/144 FPS game loops) using
//!   backtracking register allocation, alias analysis, and IEEE fast-math.

use cranelift_codegen::settings::{self, Configurable};

use crate::codegen::target::{Arch, TargetInfo, VectorExtension};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JitCompilationTier {
    /// Sub-millisecond compilation for interactive scripting, REPL, and in-game hot reload.
    FastCompile,
    /// Peak runtime speed for physics, ray tracing, animation, and game loops.
    #[default]
    MaxSpeed,
}

impl JitCompilationTier {
    pub fn from_env() -> Self {
        if let Ok(val) = std::env::var("FORGEN_JIT_TIER") {
            match val.to_lowercase().as_str() {
                "fast" | "fast_compile" | "quick" => JitCompilationTier::FastCompile,
                _ => JitCompilationTier::MaxSpeed,
            }
        } else {
            JitCompilationTier::MaxSpeed
        }
    }
}

/// Configures Cranelift compiler flags for the specified JIT tier.
pub fn configure_cranelift_flags(
    flag_builder: &mut settings::Builder,
    tier: JitCompilationTier,
    is_jit: bool,
    is_windows: bool,
) -> Result<(), String> {
    let opt_level = match tier {
        JitCompilationTier::FastCompile => "none",
        JitCompilationTier::MaxSpeed => "speed",
    };
    flag_builder
        .set("opt_level", opt_level)
        .map_err(|e| e.to_string())?;

    let is_pic = if is_jit || is_windows {
        "false"
    } else {
        "true"
    };
    flag_builder
        .set("is_pic", is_pic)
        .map_err(|e| e.to_string())?;

    // Free RBP as a general-purpose register for game calculations
    let _ = flag_builder.set("preserve_frame_pointers", "false");

    // Fast-Math IEEE-754: turn off NaN canonicalization to unlock raw hardware FPU/SIMD speed
    let _ = flag_builder.set("enable_nan_canonicalization", "false");

    // Configure register allocation algorithm based on tier
    match tier {
        JitCompilationTier::FastCompile => {
            let _ = flag_builder.set("regalloc_algorithm", "single_pass");
            let _ = flag_builder.set("enable_verifier", "false");
            let _ = flag_builder.set("enable_alias_analysis", "false");
        }
        JitCompilationTier::MaxSpeed => {
            let _ = flag_builder.set("regalloc_algorithm", "backtracking");
            let _ = flag_builder.set("enable_alias_analysis", "true");
            let _ = flag_builder.set("enable_verifier", "false");
        }
    }

    Ok(())
}

/// Enables CPU hardware acceleration features based on the target architecture.
pub fn configure_isa_hardware_features(
    isa_builder: &mut cranelift_codegen::isa::Builder,
    target: &TargetInfo,
) {
    if matches!(target.arch, Arch::X86_64) {
        let allow_sse3 =
            target.cpu_features.contains("sse3") || target.cpu_features.contains("avx2");
        let allow_sse4 =
            target.cpu_features.contains("sse4_2") || target.cpu_features.contains("avx2");
        let allow_avx = target.vector_support.contains(&VectorExtension::Avx)
            || target.vector_support.contains(&VectorExtension::Avx2);
        let allow_avx2 = target.vector_support.contains(&VectorExtension::Avx2);

        if allow_sse3 {
            let _ = isa_builder.set("has_sse3", "true");
            let _ = isa_builder.set("has_ssse3", "true");
        }
        if allow_sse4 {
            let _ = isa_builder.set("has_sse41", "true");
            let _ = isa_builder.set("has_sse42", "true");
            let _ = isa_builder.set("has_popcnt", "true");
        }
        if allow_avx {
            let _ = isa_builder.set("has_avx", "true");
        }
        if allow_avx2 {
            let _ = isa_builder.set("has_avx2", "true");
            let _ = isa_builder.set("has_fma", "true");
            let _ = isa_builder.set("has_bmi1", "true");
            let _ = isa_builder.set("has_bmi2", "true");
            let _ = isa_builder.set("has_lzcnt", "true");
        }
    }
}
