use crawfish::CompilerContext;
use crawfish::bench_tokenize;

fn main() {
    divan::main();
}

const SMALL: &str = include_str!("fixtures/small.crw");
const MEDIUM: &str = include_str!("fixtures/medium.crw");
const LARGE: &str = include_str!("fixtures/large.crw");

#[divan::bench(
    name = "find capacity to pre-allocate/small",
    args = [0, 1, 2, 3, 4, 8],
)]
fn bench_small(cap_divisor: usize) -> usize {
    let cap = if cap_divisor == 0 { 0 } else { SMALL.len() / cap_divisor };
    bench_tokenize(SMALL, &mut CompilerContext::new(), cap)
}

#[divan::bench(
    name = "find capacity to pre-allocate/medium",
    args = [0, 1, 2, 3, 4, 8],
)]
fn bench_medium(cap_divisor: usize) -> usize {
    let cap = if cap_divisor == 0 { 0 } else { MEDIUM.len() / cap_divisor };
    bench_tokenize(MEDIUM, &mut CompilerContext::new(), cap)
}

#[divan::bench(
    name = "find capacity to pre-allocate/large",
    args = [0, 1, 2, 3, 4, 8],
)]
fn bench_large(cap_divisor: usize) -> usize {
    let cap = if cap_divisor == 0 { 0 } else { LARGE.len() / cap_divisor };
    bench_tokenize(LARGE, &mut CompilerContext::new(), cap)
}
