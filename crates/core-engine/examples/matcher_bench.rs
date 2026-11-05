//! Run after matcher changes: cargo run --release -p challa-core --example matcher_bench.
use challa_core::{Snippet, SnippetMatcher};
use std::{hint::black_box, time::Instant};
fn main() {
    let mut results = Vec::new();
    for size in [100, 1_000, 10_000] {
        let rows: Vec<_> = (0..size)
            .map(|n| {
                Snippet::plain_text(
                    n.to_string(),
                    "벤치마크",
                    format!(":bench{n:05}"),
                    "평문 예제",
                )
            })
            .collect();
        let start = Instant::now();
        let matcher = SnippetMatcher::from_snippets(rows.clone());
        let batch_us = start.elapsed().as_micros();
        let start = Instant::now();
        for _ in 0..100_000 {
            black_box(matcher.find_match(black_box(":bench00099")));
        }
        let ns_per_match = start.elapsed().as_nanos() / 100_000;
        let repeated_us = if size <= 1_000 {
            let start = Instant::now();
            let mut repeated = SnippetMatcher::new();
            for row in rows {
                repeated.add_snippet(row);
            }
            Some(start.elapsed().as_micros())
        } else {
            None
        };
        results.push(serde_json::json!({"snippets":size,"batch_build_us":batch_us,"repeated_build_us":repeated_us,"average_match_ns":ns_per_match}));
    }
    println!("{}", serde_json::to_string_pretty(&results).unwrap());
}
