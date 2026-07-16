use crate::common::context::CompilerContext;
use crate::front_end::lexical_analysis::token_tree_parser::TokenTreeParser;
use crate::front_end::lexical_analysis::tokenizer::Tokenizer;
use crate::front_end::semantic_analysis::hir_dumper::HirDumper;
use crate::front_end::semantic_analysis::semantic_analyzer::SemanticAnalyzer;
use crate::front_end::syntactic_analysis::ast_dumper::AstDumper;
use crate::front_end::syntactic_analysis::parser::Parser;

/// Compiles `source` named `filename`, printing any diagnostics to stderr.
///
/// Runs lexical, syntactic, and semantic analysis in sequence, stopping (and
/// reporting) at the first stage that fails.
///
/// # Examples
///
/// ```rust,ignore
/// crawfish::driver::compile("example.crab", "func main() -> I32 { return 0; }");
/// ```
pub fn compile(filename: &str, source: &str) {
    let mut ctx = CompilerContext::new();

    // lexical analysis
    let mut tokenizer = Tokenizer::new(source, &mut ctx);
    let tokens = tokenizer.tokenize();
    let token_trees = match TokenTreeParser::new(tokens).parse() {
        Ok(trees) => trees,
        Err(diagnostics) => {
            for d in &diagnostics {
                d.report(filename, source);
            }
            return;
        }
    };

    // syntactic analysis
    let ast = match Parser::new(source, &token_trees, &ctx).parse() {
        Ok(ast) => ast,
        Err(diagnostics) => {
            for d in &diagnostics {
                d.report(filename, source);
            }
            return;
        }
    };
    #[cfg(debug_assertions)]
    match AstDumper::new(&ast, &ctx).dump() {
        Ok(output) => println!("{}", output),
        Err(e) => eprintln!("Error dumping AST: {}", e),
    };

    // semantic analysis
    let hir = match SemanticAnalyzer::new(&ast, &mut ctx).analyze() {
        Ok(result) => result,
        Err(diagnostics) => {
            for d in &diagnostics {
                d.report(filename, source);
            }
            return;
        }
    };
    #[cfg(debug_assertions)]
    match HirDumper::new(&hir, &ctx).dump() {
        Ok(output) => print!("{}", output),
        Err(e) => eprintln!("Error dumping HIR: {}", e),
    };
}

/// Type-checks a source file without generating code, reporting diagnostics
/// but not compiling further. Unimplemented.
pub fn check() {
    todo!()
}
