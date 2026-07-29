//! End-to-end tests: drive the crate through its one real public entry
//! point ([`crawfish::driver::compile`]), then actually *run* the resulting
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

    crawfish::driver::compile(path.clone());

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
    crawfish::driver::compile(path.clone());

    let executable_path = path.with_extension("");
    assert!(
        !executable_path.exists(),
        "no executable should exist when the source defines no `main`"
    );

    let _ = std::fs::remove_file(&path);
}
