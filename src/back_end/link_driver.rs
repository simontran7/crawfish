//! Turns a finished LLVM [`Module`] into an executable ahead-of-time.

use std::path::Path;
use std::process::Command;

use inkwell::OptimizationLevel;
use inkwell::module::Module;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};

/// Compiles a finished LLVM module to a native executable: emits an object
/// file alongside the target, links that into the final binary, then removes
/// the intermediate object file.
pub(crate) struct LinkDriver<'a, 'ctx> {
    module: &'a Module<'ctx>,
    executable_path: &'a Path,
}

/// Turns an object file into an executable. An abstraction point for swapping
/// the linking mechanism (e.g. `cc` vs. invoking `lld` directly) without
/// touching [`LinkDriver`].
trait LinkBackend {
    fn link(&self, object_path: &Path, executable_path: &Path) -> Result<(), String>;
}

/// Links by invoking the system's C compiler as a linker driver — the same
/// thing rustc does by default. `cc` already knows how to find the
/// platform's C runtime startup objects and libc, which a bare `ld`
/// invocation would leave to the caller.
struct Cc;

impl<'a, 'ctx> LinkDriver<'a, 'ctx> {
    pub(crate) fn new(module: &'a Module<'ctx>, executable_path: &'a Path) -> Self {
        Self {
            module,
            executable_path,
        }
    }

    pub(crate) fn link(self) -> Result<(), String> {
        let object_path = self.executable_path.with_extension("o");
        write_object_file(self.module, &object_path)?;
        Cc.link(&object_path, self.executable_path)?;
        // best-effort: a leftover .o doesn't invalidate the executable we just linked
        let _ = std::fs::remove_file(&object_path);
        Ok(())
    }
}

/// Emits `module` as a native object file at `object_path`, targeting the
/// host machine.
fn write_object_file(module: &Module, object_path: &Path) -> Result<(), String> {
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|e| format!("failed to initialize native target: {e}"))?;

    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    let target_machine = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::None,
            RelocMode::Default,
            CodeModel::Default,
        )
        .ok_or_else(|| "failed to create a target machine for the host".to_string())?;

    target_machine
        .write_to_file(module, FileType::Object, object_path)
        .map_err(|e| e.to_string())
}

impl LinkBackend for Cc {
    fn link(&self, object_path: &Path, executable_path: &Path) -> Result<(), String> {
        let status = Command::new("cc")
            .arg(object_path)
            .arg("-o")
            .arg(executable_path)
            .status()
            .map_err(|e| format!("failed to invoke the system linker (`cc`): {e}"))?;

        if status.success() {
            Ok(())
        } else {
            Err(format!("linker exited with {status}"))
        }
    }
}
