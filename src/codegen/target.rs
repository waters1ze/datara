use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arch {
    X86_64,
    Aarch64,
    RiscV64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Os {
    Windows,
    Linux,
    MacOS,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Abi {
    Msvc,
    Gnu,
    SysV,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallingConvention {
    WindowsFastcall,
    SystemV,
    Aarch64Standard,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VectorExtension {
    Sse2,
    Sse4_2,
    Avx,
    Avx2,
    Avx512,
    Neon,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Endianness {
    Little,
    Big,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetInfo {
    pub arch: Arch,
    pub os: Os,
    pub abi: Abi,
    pub pointer_width: usize,
    pub endianness: Endianness,
    pub vector_support: Vec<VectorExtension>,
    pub atomic_support: bool,
    pub calling_convention: CallingConvention,
    pub cpu_features: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionVariant {
    Generic,
    FastAvx2,
    FastNeon,
    Fallback,
}

impl TargetInfo {
    pub fn x86_64_windows() -> Self {
        let mut features = HashSet::new();
        features.insert("sse2".to_string());
        features.insert("avx2".to_string());
        features.insert("fma".to_string());
        Self {
            arch: Arch::X86_64,
            os: Os::Windows,
            abi: Abi::Msvc,
            pointer_width: 64,
            endianness: Endianness::Little,
            vector_support: vec![
                VectorExtension::Sse2,
                VectorExtension::Avx,
                VectorExtension::Avx2,
            ],
            atomic_support: true,
            calling_convention: CallingConvention::WindowsFastcall,
            cpu_features: features,
        }
    }

    pub fn x86_64_linux() -> Self {
        let mut features = HashSet::new();
        features.insert("sse2".to_string());
        features.insert("avx2".to_string());
        features.insert("fma".to_string());
        Self {
            arch: Arch::X86_64,
            os: Os::Linux,
            abi: Abi::Gnu,
            pointer_width: 64,
            endianness: Endianness::Little,
            vector_support: vec![
                VectorExtension::Sse2,
                VectorExtension::Avx,
                VectorExtension::Avx2,
            ],
            atomic_support: true,
            calling_convention: CallingConvention::SystemV,
            cpu_features: features,
        }
    }

    pub fn aarch64_windows() -> Self {
        let mut features = HashSet::new();
        features.insert("neon".to_string());
        Self {
            arch: Arch::Aarch64,
            os: Os::Windows,
            abi: Abi::Msvc,
            pointer_width: 64,
            endianness: Endianness::Little,
            vector_support: vec![VectorExtension::Neon],
            atomic_support: true,
            calling_convention: CallingConvention::Aarch64Standard,
            cpu_features: features,
        }
    }

    pub fn aarch64_linux() -> Self {
        let mut features = HashSet::new();
        features.insert("neon".to_string());
        Self {
            arch: Arch::Aarch64,
            os: Os::Linux,
            abi: Abi::Gnu,
            pointer_width: 64,
            endianness: Endianness::Little,
            vector_support: vec![VectorExtension::Neon],
            atomic_support: true,
            calling_convention: CallingConvention::Aarch64Standard,
            cpu_features: features,
        }
    }

    pub fn aarch64_macos() -> Self {
        let mut features = HashSet::new();
        features.insert("neon".to_string());
        Self {
            arch: Arch::Aarch64,
            os: Os::MacOS,
            abi: Abi::SysV,
            pointer_width: 64,
            endianness: Endianness::Little,
            vector_support: vec![VectorExtension::Neon],
            atomic_support: true,
            calling_convention: CallingConvention::Aarch64Standard,
            cpu_features: features,
        }
    }

    pub fn x86_64_macos() -> Self {
        let mut features = HashSet::new();
        features.insert("sse2".to_string());
        features.insert("avx2".to_string());
        features.insert("fma".to_string());
        Self {
            arch: Arch::X86_64,
            os: Os::MacOS,
            abi: Abi::SysV,
            pointer_width: 64,
            endianness: Endianness::Little,
            vector_support: vec![
                VectorExtension::Sse2,
                VectorExtension::Avx,
                VectorExtension::Avx2,
            ],
            atomic_support: true,
            calling_convention: CallingConvention::SystemV,
            cpu_features: features,
        }
    }

    pub fn generic_x86_64(os: Os, abi: Abi) -> Self {
        let mut features = HashSet::new();
        features.insert("sse2".to_string());
        let calling_conv = match os {
            Os::Windows => CallingConvention::WindowsFastcall,
            _ => CallingConvention::SystemV,
        };
        Self {
            arch: Arch::X86_64,
            os,
            abi,
            pointer_width: 64,
            endianness: Endianness::Little,
            vector_support: vec![VectorExtension::Sse2],
            atomic_support: true,
            calling_convention: calling_conv,
            cpu_features: features,
        }
    }

    pub fn generic_aarch64(os: Os) -> Self {
        let mut features = HashSet::new();
        features.insert("neon".to_string());
        let abi = match os {
            Os::Windows => Abi::Msvc,
            Os::MacOS => Abi::SysV,
            Os::Linux => Abi::Gnu,
        };
        Self {
            arch: Arch::Aarch64,
            os,
            abi,
            pointer_width: 64,
            endianness: Endianness::Little,
            vector_support: vec![VectorExtension::Neon],
            atomic_support: true,
            calling_convention: CallingConvention::Aarch64Standard,
            cpu_features: features,
        }
    }

    pub fn host() -> Self {
        #[allow(unused_mut)]
        let mut target = if cfg!(target_os = "windows") {
            if cfg!(target_arch = "x86_64") {
                Self::x86_64_windows()
            } else {
                Self::aarch64_windows()
            }
        } else if cfg!(target_os = "linux") {
            if cfg!(target_arch = "x86_64") {
                Self::x86_64_linux()
            } else {
                Self::aarch64_linux()
            }
        } else if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                Self::aarch64_macos()
            } else {
                Self::x86_64_macos()
            }
        } else {
            Self::x86_64_windows()
        };

        #[cfg(target_arch = "x86_64")]
        {
            let mut features = HashSet::new();
            let mut vectors = vec![VectorExtension::Sse2];
            features.insert("sse2".to_string());

            if std::is_x86_feature_detected!("sse4.2") {
                features.insert("sse4_2".to_string());
                vectors.push(VectorExtension::Sse4_2);
            }
            if std::is_x86_feature_detected!("avx") {
                features.insert("avx".to_string());
                vectors.push(VectorExtension::Avx);
            }
            if std::is_x86_feature_detected!("avx2") {
                features.insert("avx2".to_string());
                vectors.push(VectorExtension::Avx2);
            }
            if std::is_x86_feature_detected!("fma") {
                features.insert("fma".to_string());
            }
            if std::is_x86_feature_detected!("avx512f") {
                features.insert("avx512f".to_string());
                vectors.push(VectorExtension::Avx512);
            }
            target.cpu_features = features;
            target.vector_support = vectors;
        }

        target
    }

    pub fn triple_string(&self) -> String {
        let arch_str = match self.arch {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "aarch64",
            Arch::RiscV64 => "riscv64",
        };
        match self.os {
            Os::Windows => {
                let abi_str = match self.abi {
                    Abi::Msvc => "msvc",
                    _ => "gnu",
                };
                format!("{}-pc-windows-{}", arch_str, abi_str)
            }
            Os::Linux => {
                let abi_str = match self.abi {
                    Abi::Gnu => "gnu",
                    _ => "gnu",
                };
                format!("{}-unknown-linux-{}", arch_str, abi_str)
            }
            Os::MacOS => format!("{}-apple-darwin", arch_str),
        }
    }

    pub fn select_version_variant(&self) -> VersionVariant {
        if self.vector_support.contains(&VectorExtension::Avx2) {
            VersionVariant::FastAvx2
        } else if self.vector_support.contains(&VectorExtension::Neon) {
            VersionVariant::FastNeon
        } else {
            VersionVariant::Generic
        }
    }
}
