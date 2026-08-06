//! End-to-end tests: drive the crate through its one real public entry
//! point ([`crawfish::cli::driver::build`]), then actually *run* the resulting
//! executable and check its exit code. Unlike everything under `src/`
//! (unit/snapshot tests colocated with the code they test, exercising
//! `pub(crate)` internals directly), these test the whole pipeline the way
//! an actual crawfish user experiences it: source text in, a real running
//! program out.

use std::path::Path;
use std::process::Command;

/// Compiles `source` under a temporary filename, runs the resulting
/// executable, and returns its exit code. Panics if compilation didn't
/// produce a runnable executable at all, so a failure here always points at
/// the pipeline rather than a mis-asserted exit code.
fn compile_and_run(test_name: &str, source: &str) -> i32 {
    let path = std::env::temp_dir().join(format!("crawfish_e2e_{test_name}.crw"));
    std::fs::write(&path, source).expect("failed to write test source file");

    crawfish::cli::driver::build(path.clone(), &[]);

    let executable_path = path.with_extension("");
    let status = Command::new(&executable_path).status().unwrap_or_else(|e| {
        panic!("failed to run compiled executable at {executable_path:?}: {e}")
    });

    cleanup(&path, &executable_path);

    status
        .code()
        .expect("executable was terminated by a signal, not a normal exit")
}

fn cleanup(source_path: &Path, executable_path: &Path) {
    let _ = std::fs::remove_file(source_path);
    let _ = std::fs::remove_file(executable_path);
    let _ = std::fs::remove_file(executable_path.with_extension("o"));
}

#[test]
fn arithmetic_and_recursion_produce_the_correct_exit_code() {
    let source = r#"
        func fib(n: I32) -> I32 {
            if n < 2 {
                return n;
            }
            fib(n - 1) + fib(n - 2)
        }

        func main() -> I32 {
            fib(10)
        }
    "#;
    assert_eq!(compile_and_run("fib", source), 55);
}

/// Exercises the merge-block-parameter lowering for both `if`-as-a-value and
/// `and`/`or` directly: `then_val`/`else_val` check that each arm's value
/// reaches the merge block correctly, and `and_skips`/`or_skips` guard a
/// division by zero that only a genuinely short-circuited rhs avoids running
/// — if short-circuiting were broken, this test would crash (SIGFPE) rather
/// than merely assert a wrong number.
#[test]
fn if_expressions_and_short_circuit_operators_evaluate_correctly() {
    let source = r#"
        func choose(cond: Bool, a: I32, b: I32) -> I32 {
            if cond { a } else { b }
        }

        func main() -> I32 {
            let x: I32 = 0;
            let y: I32 = 5;

            let then_val: I32 = choose(true, 100, 200);
            let else_val: I32 = choose(false, 100, 200);

            let and_skips: Bool = (x != 0) and (1 / x > 0);
            let and_evaluates: Bool = (y != 0) and (10 / y == 2);
            let or_skips: Bool = (x == 0) or (1 / x > 0);
            let or_evaluates: Bool = (y == 0) or (10 / y == 2);

            let mut score: I32 = 0;
            if then_val == 100 {
                score = score + 1;
            }
            if else_val == 200 {
                score = score + 1;
            }
            if and_skips == false {
                score = score + 1;
            }
            if and_evaluates == true {
                score = score + 1;
            }
            if or_skips == true {
                score = score + 1;
            }
            if or_evaluates == true {
                score = score + 1;
            }
            score
        }
    "#;
    assert_eq!(compile_and_run("if_and_short_circuit", source), 6);
}

#[test]
fn unsigned_comparison_and_division_dont_use_the_signed_bit_pattern() {
    let source = r#"
        func main() -> I32 {
            let big: U32 = 4000000000;
            let small: U32 = 2;

            let mut result: I32 = 0;
            if big > small {
                result = result + 1;
            }
            if big / small > small {
                result = result + 10;
            }
            result
        }
    "#;
    // `big` (4_000_000_000) has its top bit set, so it reads as negative
    // under a *signed* 32-bit interpretation. Both checks only pass if the
    // comparison and division are done unsigned: `big > small` would be
    // false, and `big / small` would truncate wrong, under a signed
    // interpretation.
    assert_eq!(compile_and_run("unsigned_ops", source), 11);
}

#[test]
fn less_equal_and_greater_equal_are_inclusive_at_the_boundary() {
    let source = r#"
        func clamp(x: I32, lo: I32, hi: I32) -> I32 {
            if x <= lo {
                return lo;
            }
            if x >= hi {
                return hi;
            }
            x
        }

        func main() -> I32 {
            clamp(5, 5, 10) + clamp(10, 5, 10) + clamp(7, 5, 10)
        }
    "#;
    // clamp(5, 5, 10) = 5 (x <= lo is true only if `<=` is inclusive)
    // clamp(10, 5, 10) = 10 (x >= hi is true only if `>=` is inclusive)
    // clamp(7, 5, 10) = 7 (neither boundary hit)
    assert_eq!(compile_and_run("clamp", source), 22);
}

#[test]
fn zero_sized_arguments_still_run_their_side_effects() {
    let source = r#"
        func ignore(a: I32, u: Unit, b: I32) -> I32 {
            a + b
        }

        func main() -> I32 {
            let mut counter: I32 = 7;
            let total: I32 = ignore(1, counter = counter + 5, 2);
            total + counter
        }
    "#;
    // ignore(1, .., 2) = 3; counter mutates to 12 via the erased argument's
    // side effect; 3 + 12 = 15.
    assert_eq!(compile_and_run("zero_sized", source), 15);
}

#[test]
fn a_source_file_with_no_main_fails_to_link_instead_of_panicking() {
    let path = std::env::temp_dir().join("crawfish_e2e_no_main.crw");
    let source = "func helper() -> I32 { 1 }";
    std::fs::write(&path, source).expect("failed to write test source file");

    // Must not panic: today this is reported as a linker error rather than a
    // dedicated diagnostic (see the driver's `Error producing executable`
    // message), but the driver itself has to survive it either way.
    crawfish::cli::driver::build(path.clone(), &[]);

    let executable_path = path.with_extension("");
    assert!(
        !executable_path.exists(),
        "no executable should exist when the source defines no `main`"
    );

    let _ = std::fs::remove_file(&path);
}
