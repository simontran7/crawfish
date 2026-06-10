# Style Guide

## Assertions

Use `assert!` by default. Only use `debug_assert!` if you have benchmarked the code and identified that a specific assertion causes a measurable performance regression (for example, a costly check inside a hot loop).

This is because assertions are almost never a detectable performance hit unless they appear in a hot loop or perform particularly complex work. A well-placed `assert!` can even *improve* performance, because the compiler may use it to eliminate downstream redundant checks (a common trick for helping autovectorization: asserting slice lengths before a loop removes per-iteration bounds checks).

More importantly, failing loudly in release builds is better than failing silently. A compiler that panics with a clear message is better than one that produces wrong output.

## Code Layout

https://www.reddit.com/r/rust/comments/wwbxhw/comment/ilkid50/?screen_view_count=4
