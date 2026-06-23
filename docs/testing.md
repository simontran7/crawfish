# Testing

## Approach

Following [matklad's testing philosophy](https://matklad.github.io/2021/05/31/how-to-test.html), crawfish uses **integrated snapshot tests** as its primary testing strategy. Compilers are pure self-contained functions (source in, structured output out), which makes them ideal for this approach.

Each test feeds a `.crw` source file through the full pipeline up to some stage, snapshots the output, and compares it against a saved baseline. This tests features, not code: the tests are independent of internal APIs, so refactoring internals doesn't break them. Adding a new test case is just adding a new `.crw` file.

Unit tests are reserved for isolated algorithmic code where integrated tests wouldn't catch edge cases (e.g., the `ValueListAllocator`, `UnificationTable`).

## Snapshot Tests with insta

Crawfish uses [insta](https://github.com/mitsuhiko/insta) for [snapshot testing](https://www.cs.cornell.edu/~asampson/blog/turnt.html). Each compilation stage has a single test that globs over all `.crw` input files in its `inputs/` directory, runs the pipeline, and snapshots the result.

### Adding a test

1. Create an input file `.crw` in `lexical_analysis/inputs/`, `syntactic_analysis/inputs/`, or `semantic_analysis/inputs/`.

2. Run the test (it will fail because no snapshot exists yet).

3. Review the snapshot using `cargo insta review` to make sure the output looks correct. If it does, accept it.

4. Commit the `.snap` file to git. This is what makes it a regression test going forward.
