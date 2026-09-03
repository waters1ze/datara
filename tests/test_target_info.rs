use forgen::codegen::target::{
    Arch, CallingConvention, Os, TargetInfo, VectorExtension, VersionVariant,
};

#[test]
fn test_target_info_definitions() {
    let win_x64 = TargetInfo::x86_64_windows();
    assert_eq!(win_x64.arch, Arch::X86_64);
    assert_eq!(win_x64.os, Os::Windows);
    assert_eq!(
        win_x64.calling_convention,
        CallingConvention::WindowsFastcall
    );
    assert!(win_x64.vector_support.contains(&VectorExtension::Avx2));
    assert_eq!(win_x64.triple_string(), "x86_64-pc-windows-msvc");

    let linux_x64 = TargetInfo::x86_64_linux();
    assert_eq!(linux_x64.arch, Arch::X86_64);
    assert_eq!(linux_x64.os, Os::Linux);
    assert_eq!(linux_x64.calling_convention, CallingConvention::SystemV);
    assert_eq!(linux_x64.triple_string(), "x86_64-unknown-linux-gnu");

    let arm_linux = TargetInfo::aarch64_linux();
    assert_eq!(arm_linux.arch, Arch::Aarch64);
    assert_eq!(
        arm_linux.calling_convention,
        CallingConvention::Aarch64Standard
    );
    assert!(arm_linux.vector_support.contains(&VectorExtension::Neon));
    assert_eq!(arm_linux.triple_string(), "aarch64-unknown-linux-gnu");

    let mac_arm = TargetInfo::aarch64_macos();
    assert_eq!(mac_arm.arch, Arch::Aarch64);
    assert_eq!(mac_arm.os, Os::MacOS);
    assert_eq!(mac_arm.triple_string(), "aarch64-apple-darwin");

    let mac_x64 = TargetInfo::x86_64_macos();
    assert_eq!(mac_x64.arch, Arch::X86_64);
    assert_eq!(mac_x64.os, Os::MacOS);
    assert_eq!(mac_x64.triple_string(), "x86_64-apple-darwin");
}

#[test]
fn test_version_variant_dispatch() {
    let win_x64 = TargetInfo::x86_64_windows();
    assert_eq!(win_x64.select_version_variant(), VersionVariant::FastAvx2);

    let arm_linux = TargetInfo::aarch64_linux();
    assert_eq!(arm_linux.select_version_variant(), VersionVariant::FastNeon);

    let generic_win = TargetInfo::generic_x86_64(Os::Windows, forgen::codegen::target::Abi::Msvc);
    assert_eq!(generic_win.select_version_variant(), VersionVariant::Generic);
    assert!(generic_win.vector_support.contains(&VectorExtension::Sse2));
    assert!(!generic_win.vector_support.contains(&VectorExtension::Avx2));

    let host = TargetInfo::host();
    assert!(host.pointer_width == 64);
    assert!(!host.triple_string().is_empty());
}
