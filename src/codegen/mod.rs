use crate::ast::Program;
use crate::dmir::Module;
use crate::types::TypeChecker;
use std::path::{Path, PathBuf};

pub mod cranelift;
pub mod linker;
pub mod target;

pub use cranelift::{
    CraneliftBackend, FunctionCodegenInspection, ModuleCodegenInspection, RealCraneliftBackend,
};
pub use target::TargetInfo;

/// Common trait implemented by native code generation backends in Forgen.
pub trait CodegenBackend: Send + Sync {
    fn emit(&self, module: &Module, program: &Program, types: &TypeChecker) -> String;
    fn compile_to_executable(&self, source: &str, target_path: &Path) -> Result<PathBuf, String>;
    fn target_info(&self) -> TargetInfo;
    fn run_executable(
        &self,
        exe_path: &Path,
        args: &[String],
    ) -> Result<(String, String, i32, u128), String>;
}

/// Native codegen alias pointing to the primary Cranelift backend.
pub type NativeCodegen = RealCraneliftBackend;
