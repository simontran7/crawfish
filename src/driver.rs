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

use std::path::Path;

use inkwell::context::Context;

/// Compiles `source` named `filename`, printing any diagnostics to stderr.
///
/// Every stage adds what it finds to [`CompilerContext::diagnostics`] and
/// keeps going, so a stage reports everything it found rather than bailing at
/// the first problem. The pipeline stops at the first stage boundary where an
/// error (as opposed to a warning) was recorded.
///
/// # Examples
///
/// ```rust,ignore
/// crawfish::driver::compile("example.crw", "func main() -> I32 { return 0; }");
/// ```
pub fn compile(filename: &str, source: &str) {
    let mut ctx = CompilerContext::new();

    // lexical analysis
    let mut tokenizer = Tokenizer::new(source, &mut ctx);
    let tokens = tokenizer.tokenize();
    let token_trees = TokenTreeParser::new(tokens, &ctx).parse();
    if ctx.diagnostics.has_errors() {
        ctx.diagnostics.render(filename, source);
        return;
    }

    // syntactic analysis
    let ast = Parser::new(source, &token_trees, &ctx).parse();
    if ctx.diagnostics.has_errors() {
        ctx.diagnostics.render(filename, source);
        return;
    }
    #[cfg(debug_assertions)]
    match AstDumper::new(&ast, &ctx).dump() {
        Ok(output) => println!("{}", output),
        Err(e) => eprintln!("Error dumping AST: {}", e),
    };

    // semantic analysis
    let hir = SemanticAnalyzer::new(&ast, &mut ctx).analyze();
    if ctx.diagnostics.has_errors() {
        ctx.diagnostics.render(filename, source);
        return;
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
        ctx.diagnostics.render(filename, source);
        return;
    }
    #[cfg(debug_assertions)]
    match MirDumper::new(&mir, &ctx).dump() {
        Ok(output) => print!("{}", output),
        Err(e) => eprintln!("Error dumping MIR: {}", e),
    };

    let llvm_context = Context::create();
    let module = LlvmCodegen::new(&mir, &ctx, &llvm_context, filename).compile();
    #[cfg(debug_assertions)]
    println!("{}", module.print_to_string().to_string());

    // ahead-of-time: object code + link, producing a real, standalone
    // executable — not a JIT, matching how rustc/Zig/Go all deliver a build
    let executable_path = Path::new(filename).with_extension("");
    if let Err(e) = target::compile_to_executable(&module, &executable_path) {
        eprintln!("Error producing executable: {e}");
    }

    ctx.diagnostics.render(filename, source);
}

/// Checks a source file without generating code.
pub fn check() {
    todo!()
}
