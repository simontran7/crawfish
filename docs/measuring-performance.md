# Measuring Performance

1. Write a Criterion benchmark that tokenizes a representative .crw file
2. Profile it with samply to see where time is spent
3. Swap `Vec<Token>` for `SmallVec<Token>`, re-run the benchmark, and compare
