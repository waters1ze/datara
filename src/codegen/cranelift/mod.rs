pub mod backend;
pub mod clif;

use crate::ast::Program;
use crate::codegen::CodegenBackend;
use crate::codegen::target::TargetInfo;
use crate::dmir::Module;
use crate::types::TypeChecker;
use std::path::{Path, PathBuf};

pub use self::backend::RealCraneliftBackend;
pub use self::clif::{ClifEmitter, FunctionCodegenInspection, ModuleCodegenInspection};

pub struct CraneliftBackend {
    pub target: TargetInfo,
    pub real_backend: RealCraneliftBackend,
}

impl CraneliftBackend {
    pub fn new(target: TargetInfo) -> Self {
        Self {
            target: target.clone(),
            real_backend: RealCraneliftBackend::new(target),
        }
    }

    pub fn for_host() -> Self {
        Self::new(TargetInfo::host())
    }

    pub fn emit_clif(&self, module: &Module, program: &Program, types: &TypeChecker) -> String {
        let emitter = ClifEmitter::new(&self.target);
        emitter.emit_module(module, program, types)
    }

    pub fn inspect_module(&self, module: &Module) -> ModuleCodegenInspection {
        let emitter = ClifEmitter::new(&self.target);
        emitter.inspect_module(module)
    }

    pub fn compile_native(
        &self,
        dmir_module: &Module,
        output_path: &Path,
    ) -> Result<PathBuf, String> {
        let exports: Vec<String> = dmir_module.functions.keys().cloned().collect();
        let obj_bytes = self.real_backend.compile_to_object_bytes(dmir_module)?;
        self.real_backend
            .link_object_to_executable(&obj_bytes, output_path, &exports)
    }

    pub fn run_executable(
        &self,
        exe_path: &Path,
        args: &[String],
    ) -> Result<(String, String, i32, u128), String> {
        self.real_backend.run_executable(exe_path, args)
    }
}

impl CodegenBackend for CraneliftBackend {
    fn target_info(&self) -> TargetInfo {
        self.target.clone()
    }

    fn emit(&self, module: &Module, program: &Program, types: &TypeChecker) -> String {
        self.emit_clif(module, program, types)
    }

    fn compile_to_executable(&self, _source: &str, _output_path: &Path) -> Result<PathBuf, String> {
        Err("Direct source string compilation deprecated; compile via compile_native with DMIR Module".to_string())
    }

    fn run_executable(
        &self,
        exe_path: &Path,
        args: &[String],
    ) -> Result<(String, String, i32, u128), String> {
        self.real_backend.run_executable(exe_path, args)
    }
}
