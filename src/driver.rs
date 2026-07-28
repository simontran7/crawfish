use crate::common::context::CompilerContext;
use crate::front_end::lexical_analysis::token_tree_parser::TokenTreeParser;
use crate::front_end::lexical_analysis::tokenizer::Tokenizer;
use crate::front_end::semantic_analysis::hir_dumper::HirDumper;
use crate::front_end::semantic_analysis::semantic_analyzer::SemanticAnalyzer;
use crate::front_end::syntactic_analysis::ast_dumper::AstDumper;
use crate::front_end::syntactic_analysis::parser::Parser;

use crate::back_end::llvm_codegen::LlvmCodegen;
use crate::back_end::target;
use crate::middle_end::lowerer::MirLowerer;
use crate::middle_end::mir_dumper::MirDumper;

use std::{fs, path::PathBuf, process::Command};

use inkwell::context::Context;

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
    run_pipeline(path, true)
}

/// Checks `path` for diagnostics without producing an executable.
pub fn check(path: PathBuf) {
    run_pipeline(path, false);
}

/// Compiles `path`, runs the resulting executable, and removes it once it
/// finishes — `run` is for immediate feedback, not for producing a build
/// artifact, so nothing should be left behind. Forwards the executable's exit
/// code; exits with status 1 if compilation didn't produce an executable.
pub fn run(path: PathBuf) {
    let Some(executable_path) = compile(path) else {
        std::process::exit(1);
    };
    let canonical_path = executable_path.canonicalize().unwrap_or_else(|e| {
        eprintln!("Error locating executable {executable_path:?}: {e}");
        std::process::exit(1);
    });
    let status = Command::new(&canonical_path).status().unwrap_or_else(|e| {
        eprintln!("Error running executable {canonical_path:?}: {e}");
        std::process::exit(1);
    });
    let _ = std::fs::remove_file(&executable_path);
    std::process::exit(status.code().unwrap_or(1));
}

/// Runs the front end and middle end on `path`, then either stops (`emit_code
/// = false`, for [`check`]) or continues through codegen and linking
/// (`emit_code = true`, for [`compile`]), returning the executable's path on
/// full success.
fn run_pipeline(path: PathBuf, emit_code: bool) -> Option<PathBuf> {
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

    // mir lowering + mir transformation passes + llvm lowering
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

    if !emit_code {
        ctx.diagnostics.render(&filename, &source);
        let (errors, warnings) = ctx.diagnostics.counts();
        println!("\x1b[1;31mCompiler Errors: {errors}\x1b[0m");
        println!("\x1b[1;33mWarnings: {warnings}\x1b[0m");
        return None;
    }

    let llvm_context = Context::create();
    let module = LlvmCodegen::new(&mir, &ctx, &llvm_context, &filename).compile();
    #[cfg(debug_assertions)]
    println!("{}", module.print_to_string().to_string());

    // ahead-of-time: object code + link, producing a real, standalone
    // executable — not a JIT, matching how rustc/Zig/Go all deliver a build
    let executable_path = path.with_extension("");
    let link_result = target::compile_to_executable(&module, &executable_path);
    ctx.diagnostics.render(&filename, &source);
    match link_result {
        Ok(()) => Some(executable_path),
        Err(e) => {
            eprintln!("Error producing executable: {e}");
            None
        }
    }
}
