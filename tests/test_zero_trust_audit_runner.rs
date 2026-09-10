#[path = "zero_trust_audit/helpers.rs"]
pub mod helpers;

#[path = "zero_trust_audit/overflow.rs"]
mod overflow;

#[path = "zero_trust_audit/ownership.rs"]
mod ownership;

#[path = "zero_trust_audit/contracts.rs"]
mod contracts;

#[path = "zero_trust_audit/scheduler.rs"]
mod scheduler;

#[path = "zero_trust_audit/wasm_capabilities.rs"]
mod wasm_capabilities;

#[path = "zero_trust_audit/simd.rs"]
mod simd;

#[path = "zero_trust_audit/dwarf.rs"]
mod dwarf;

#[path = "zero_trust_audit/modules_traits.rs"]
mod modules_traits;

#[path = "zero_trust_audit/datara_test_runner.rs"]
mod datara_test_runner;

#[path = "zero_trust_audit/parser_unicode.rs"]
mod parser_unicode;

#[path = "zero_trust_audit/differential_12.rs"]
mod differential_12;

#[path = "zero_trust_audit/diagnostics_12.rs"]
mod diagnostics_12;

#[path = "zero_trust_audit/panic_hunt_cli.rs"]
mod panic_hunt_cli;

#[path = "zero_trust_audit/release_benchmarks.rs"]
mod release_benchmarks;

#[path = "zero_trust_audit/stage1_performance_audit.rs"]
mod stage1_performance_audit;
