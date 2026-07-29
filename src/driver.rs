use crate::common::context::CompilerContext;
use crate::front_end::lexical_analysis::token_tree_parser::TokenTreeParser;
use crate::front_end::lexical_analysis::tokenizer::Tokenizer;
use crate::front_end::semantic_analysis::hir_dumper::HirDumper;
use crate::front_end::semantic_analysis::semantic_analyzer::SemanticAnalyzer;
use crate::front_end::syntactic_analysis::ast_dumper::AstDumper;
use crate::front_end::syntactic_analysis::parser::Parser;

use crate::back_end::linker;
use crate::back_end::llvm_codegen::LlvmCodegen;
use crate::middle_end::lowerer::MirLowerer;
use crate::middle_end::mir::Mir;
use crate::middle_end::mir_dumper::MirDumper;

use std::{fs, path::PathBuf};

use inkwell::OptimizationLevel;
use inkwell::context::Context;
use inkwell::targets::{InitializationConfig, Target};

/// Compiles `path`, printing any diagnostics to stderr, and returns the path
/// to the produced executable on success.
///
/// Every stage adds what it finds to [`CompilerContext::diagnostics`] and
/// keeps going, so a stage reports everything it found rather than bailing at
/// the first problem. The pipeline stops at the first stage boundary where an
/// error (as opposed to a warning) was recorded.
///
/// # Examples
///
/// ```rust,ignore
/// crawfish::driver::compile("example.crw".into());
/// ```
pub fn compile(path: PathBuf) -> Option<PathBuf> {
    let unit = compile_to_mir(path)?;
    let llvm_context = Context::create();
    let module = LlvmCodegen::new(&unit.mir, &unit.ctx, &llvm_context, &unit.filename).compile();
    #[cfg(debug_assertions)]
    println!("{}", module.print_to_string().to_string());

    // ahead-of-time: object code + link, producing a real, standalone
    // executable — not a JIT, matching how rustc/Zig/Go all deliver a build
    let executable_path = unit.path.with_extension("");
    let link_result = linker::compile_to_executable(&module, &executable_path);
    unit.ctx.diagnostics.render(&unit.filename, &unit.source);
    match link_result {
        Ok(()) => Some(executable_path),
        Err(e) => {
            eprintln!("Error producing executable: {e}");
            None
        }
    }
}

/// Checks `path` for diagnostics without producing an executable.
pub fn check(path: PathBuf) {
    let Some(unit) = compile_to_mir(path) else {
        return;
    };
    unit.ctx.diagnostics.render(&unit.filename, &unit.source);
    let (errors, warnings) = unit.ctx.diagnostics.counts();
    println!("\x1b[1;31mCompiler Errors: {errors}\x1b[0m");
    println!("\x1b[1;33mWarnings: {warnings}\x1b[0m");
}

/// Compiles `path` and JIT-executes the result directly in this process,
/// forwarding `main`'s return value as the process exit code.
///
/// `run` is for immediate feedback, so unlike [`compile`] it never touches
/// disk: no executable file, no linker invocation, and no OS-level
/// first-launch security check paid on every call — those turned out to
/// dominate wall-clock time far more than the compiler itself.
pub fn run(path: PathBuf) {
    let Some(unit) = compile_to_mir(path) else {
        std::process::exit(1);
    };
    let llvm_context = Context::create();
    let module = LlvmCodegen::new(&unit.mir, &unit.ctx, &llvm_context, &unit.filename).compile();
    #[cfg(debug_assertions)]
    println!("{}", module.print_to_string().to_string());
    unit.ctx.diagnostics.render(&unit.filename, &unit.source);

    Target::initialize_native(&InitializationConfig::default())
        .expect("failed to initialize native target for JIT execution");
    let execution_engine = module
        .create_jit_execution_engine(OptimizationLevel::None)
        .unwrap_or_else(|e| {
            eprintln!("Error creating JIT execution engine: {e}");
            std::process::exit(1);
        });
    let exit_code = unsafe {
        let main_fn = execution_engine
            .get_function::<unsafe extern "C" fn() -> i32>("main")
            .unwrap_or_else(|e| {
                eprintln!("Error locating `main`: {e:?}");
                std::process::exit(1);
            });
        main_fn.call()
    };
    std::process::exit(exit_code);
}

/// Everything [`compile_to_mir`] produces: the lowered [`Mir`] plus the
/// state needed to continue into codegen or render diagnostics.
struct CompileUnit {
    mir: Mir,
    ctx: CompilerContext,
    path: PathBuf,
    filename: String,
    source: String,
}

/// Runs the front end and middle end on `path`, returning a [`CompileUnit`].
/// Returns `None` — having already rendered diagnostics — if any stage
/// reported an error; exits directly if `path` itself couldn't be read.
fn compile_to_mir(path: PathBuf) -> Option<CompileUnit> {
    let mut ctx = CompilerContext::new();

    let filename = path.to_string_lossy().to_string();
    let source = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading file {}: {}", filename, e);
            std::process::exit(1);
        }
    };

    // lexical analysis
    let mut tokenizer = Tokenizer::new(&source, &mut ctx);
    let tokens = tokenizer.tokenize();
    let token_trees = TokenTreeParser::new(tokens, &ctx).parse();
    if ctx.diagnostics.has_errors() {
        ctx.diagnostics.render(&filename, &source);
        return None;
    }

    // syntactic analysis
    let ast = Parser::new(&source, &token_trees, &ctx).parse();
    if ctx.diagnostics.has_errors() {
        ctx.diagnostics.render(&filename, &source);
        return None;
    }
    #[cfg(debug_assertions)]
    match AstDumper::new(&ast, &ctx).dump() {
        Ok(output) => println!("{}", output),
        Err(e) => eprintln!("Error dumping AST: {}", e),
    };

    // semantic analysis
    let hir = SemanticAnalyzer::new(&ast, &mut ctx).analyze();
    if ctx.diagnostics.has_errors() {
        ctx.diagnostics.render(&filename, &source);
        return None;
    }
    #[cfg(debug_assertions)]
    match HirDumper::new(&hir, &ctx).dump() {
        Ok(output) => print!("{}", output),
        Err(e) => eprintln!("Error dumping HIR: {}", e),
    };

    // mir lowering + mir transformation passes
    //
    // Every function is lowered before anything consumes the result, so later
    // MIR passes can look across function boundaries (inlining, whole-program
    // analysis) rather than seeing one body at a time.
    let mir = MirLowerer::new(&hir, &ctx).lower();
    if ctx.diagnostics.has_errors() {
        ctx.diagnostics.render(&filename, &source);
        return None;
    }
    #[cfg(debug_assertions)]
    match MirDumper::new(&mir, &ctx).dump() {
        Ok(output) => print!("{}", output),
        Err(e) => eprintln!("Error dumping MIR: {}", e),
    };

    Some(CompileUnit {
        mir,
        ctx,
        path,
        filename,
        source,
    })
}
