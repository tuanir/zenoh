//
// Copyright (c) 2026 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

//! Benchmarks `keyexpr::intersects` and `keyexpr::includes`.
//!
//! Only the public API is used, so the same benchmark runs against any version of the matchers.
//! To compare two commits, save a baseline on the first and compare against it on the second:
//!
//! ```sh
//! git checkout <old>
//! cargo bench -p zenoh-keyexpr --bench keyexpr_match -- --save-baseline old
//! git checkout <new>
//! cargo bench -p zenoh-keyexpr --bench keyexpr_match -- --baseline old
//! ```

use std::{hint::black_box, time::Duration};

use criterion::{criterion_group, criterion_main, Criterion};
use zenoh_keyexpr::{keyexpr, OwnedKeyExpr};

/// The `intersections` test cases.
const INTERSECTIONS: &[(&str, &str)] = &[
    ("a", "a"),
    ("a/b", "a/b"),
    ("*", "abc"),
    ("*", "xxx"),
    ("ab$*", "abcd"),
    ("ab$*d", "abcd"),
    ("ab$*", "ab"),
    ("ab/*", "ab"),
    ("a/*/c/*/e", "a/b/c/d/e"),
    ("a/$*b/c/$*d/e", "a/xb/c/xd/e"),
    ("a/*/c/*/e", "a/c/e"),
    ("a/*/c/*/e", "a/b/c/d/x/e"),
    ("ab$*cd", "abxxcxxd"),
    ("ab$*cd", "abxxcxxcd"),
    ("ab$*cd", "abxxcxxcdx"),
    ("**", "abc"),
    ("**", "a/b/c"),
    ("ab/**", "ab"),
    ("**/xyz", "a/b/xyz/d/e/f/xyz"),
    ("**/xyz$*xyz", "a/b/xyz/d/e/f/xyz"),
    ("**/xyz$*xyz", "a/b/xyzdefxyz"),
    ("a/**/c/**/e", "a/b/b/b/c/d/d/d/e"),
    ("a/**/c/**/e", "a/c/e"),
    ("a/**/c/*/e/*", "a/b/b/b/c/d/d/c/d/e/f"),
    ("a/**/c/*/e/*", "a/b/b/b/c/d/d/c/d/d/e/f"),
    ("ab$*cd", "abxxcxxcdx"),
    ("x/abc", "x/abc"),
    ("x/abc", "abc"),
    ("x/*", "x/abc"),
    ("x/*", "abc"),
    ("*", "x/abc"),
    ("x/*", "x/abc$*"),
    ("x/$*abc", "x/abc$*"),
    ("x/a$*", "x/abc$*"),
    ("x/a$*de", "x/abc$*de"),
    ("x/a$*d$*e", "x/a$*e"),
    ("x/a$*d$*e", "x/a$*c$*e"),
    ("x/a$*d$*e", "x/ade"),
    ("x/c$*", "x/abc$*"),
    ("x/$*d", "x/$*e"),
    ("@a", "@a"),
    ("@a", "@ab"),
    ("@a", "@a/b"),
    ("@a", "@a/*"),
    ("@a", "@a/*/**"),
    ("@a", "@a$*/**"),
    ("@a", "@a/**"),
    ("**/xyz$*xyz", "@a/b/xyzdefxyz"),
    ("@a/**/c/**/e", "@a/b/b/b/c/d/d/d/e"),
    ("@a/**/c/**/e", "@a/@b/b/b/c/d/d/d/e"),
    ("@a/**/@c/**/e", "@a/b/b/b/@c/d/d/d/e"),
    ("@a/**/e", "@a/b/b/d/d/d/e"),
    ("@a/**/e", "@a/b/b/b/d/d/d/e"),
    ("@a/**/e", "@a/b/b/c/d/d/d/e"),
    ("@a/**/e", "@a/b/b/@c/b/d/d/d/e"),
    ("@a/*", "@a/@b"),
    ("@a/**", "@a/@b"),
    ("@a/**/@b", "@a/@b"),
    ("@a/@b/**", "@a/@b"),
    ("@a/**/@c/**/@b", "@a/**/@c/@b"),
    ("@a/**/@c/**/@b", "@a/@c/**/@b"),
    ("@a/**/@c/@b", "@a/@c/**/@b"),
    ("@a/**/@b", "@a/**/@c/**/@b"),
    ("@a", "**/@a"),
];

/// The `inclusions` test cases.
const INCLUSIONS: &[(&str, &str)] = &[
    ("a", "a"),
    ("a/b", "a/b"),
    ("*", "abc"),
    ("*", "xxx"),
    ("ab$*", "abcd"),
    ("ab$*d", "abcd"),
    ("ab$*", "ab"),
    ("ab/*", "ab"),
    ("a/*/c/*/e", "a/b/c/d/e"),
    ("a/$*b/c/$*d/e", "a/xb/c/xd/e"),
    ("a/*/c/*/e", "a/c/e"),
    ("a/*/c/*/e", "a/b/c/d/x/e"),
    ("ab$*cd", "abxxcxxd"),
    ("ab$*c$*d", "abxxcxxd"),
    ("ab$*cd", "abxxcxxcd"),
    ("ab$*cd", "abxxcxxcdx"),
    ("**", "abc"),
    ("**", "a/b/c"),
    ("ab/**", "ab"),
    ("**/xyz", "a/b/xyz/d/e/f/xyz"),
    ("**/xyz$*xyz", "a/b/xyz/d/e/f/xyz"),
    ("**/xyz$*xyz", "a/b/xyzdefxyz"),
    ("a/**/c/**/e", "a/b/b/b/c/d/d/d/e"),
    ("a/**/c/**/e", "a/c/e"),
    ("a/**/c/*/e/*", "a/b/b/b/c/d/d/c/d/e/f"),
    ("a/**/c/*/e/*", "a/b/b/b/c/d/d/c/d/d/e/f"),
    ("ab$*cd", "abxxcxxcdx"),
    ("x/abc", "x/abc"),
    ("x/abc", "abc"),
    ("x/*", "x/abc"),
    ("x/*", "abc"),
    ("*", "x/abc"),
    ("x/*", "x/abc$*"),
    ("x/$*abc", "x/abc$*"),
    ("x/a$*", "x/abc$*"),
    ("x/abc$*", "x/a$*"),
    ("x/a$*de", "x/abc$*de"),
    ("x/a$*e", "x/a$*d$*e"),
    ("x/a$*d$*e", "x/a$*e"),
    ("x/a$*d$*e", "x/a$*c$*e"),
    ("x/a$*d$*e", "x/ade"),
    ("x/c$*", "x/abc$*"),
    ("x/$*c$*", "x/abc$*"),
    ("x/$*d", "x/$*e"),
    ("@a", "@a"),
    ("@a", "@ab"),
    ("@a", "@a/b"),
    ("@a", "@a/*"),
    ("@a", "@a/*/**"),
    ("@a$*/**", "@a"),
    ("@a", "@a/**"),
    ("@a/**", "@a"),
    ("**/xyz$*xyz", "@a/b/xyzdefxyz"),
    ("@a/**/c/**/e", "@a/b/b/b/c/d/d/d/e"),
    ("@a/*", "@a/@b"),
    ("@a/**", "@a/@b"),
    ("@a/**/@b", "@a/@b"),
    ("@a/@b/**", "@a/@b"),
];

/// Pairs of single chunks, as the tree iterators compare them.
const SINGLE_CHUNKS: &[(&str, &str)] = &[
    ("abc", "abc"),
    ("abc", "abd"),
    ("*", "abc"),
    ("**", "abc"),
    ("@a", "@a"),
    ("@a", "*"),
    ("ab$*", "abcd"),
    ("$*cd", "abcd"),
    ("a$*b$*c", "axxbyyc"),
    ("a$*b$*c", "axxbyyd"),
    ("$*x$*", "a$*b"),
];

type Pairs = Vec<(OwnedKeyExpr, OwnedKeyExpr)>;

fn pairs(cases: impl IntoIterator<Item = (String, String)>) -> Pairs {
    cases
        .into_iter()
        .map(|(l, r)| (OwnedKeyExpr::new(l).unwrap(), OwnedKeyExpr::new(r).unwrap()))
        .collect()
}

fn intersect_all(pairs: &[(&keyexpr, &keyexpr)]) {
    for (l, r) in pairs {
        black_box(black_box(l).intersects(black_box(r)));
    }
}

fn include_all(pairs: &[(&keyexpr, &keyexpr)]) {
    for (l, r) in pairs {
        black_box(black_box(l).includes(black_box(r)));
    }
}

fn borrow(pairs: &Pairs) -> Vec<(&keyexpr, &keyexpr)> {
    pairs.iter().map(|(l, r)| (&**l, &**r)).collect()
}

fn test_cases(c: &mut Criterion) {
    let ints = pairs(
        INTERSECTIONS
            .iter()
            .map(|(l, r)| (l.to_string(), r.to_string())),
    );
    let incs = pairs(
        INCLUSIONS
            .iter()
            .map(|(l, r)| (l.to_string(), r.to_string())),
    );
    let (ints, incs) = (borrow(&ints), borrow(&incs));
    let mut g = c.benchmark_group("test_cases");
    g.bench_function("intersect", |b| b.iter(|| intersect_all(&ints)));
    g.bench_function("include", |b| b.iter(|| include_all(&incs)));
    g.finish();
}

fn single_chunks(c: &mut Criterion) {
    let chunks = pairs(
        SINGLE_CHUNKS
            .iter()
            .map(|(l, r)| (l.to_string(), r.to_string())),
    );
    let chunks = borrow(&chunks);
    let mut g = c.benchmark_group("single_chunks");
    g.bench_function("intersect", |b| b.iter(|| intersect_all(&chunks)));
    g.bench_function("include", |b| b.iter(|| include_all(&chunks)));
    g.finish();
}

/// Inputs that make backtracking matchers exponential, or quadratic within a chunk.
/// `n` is the number of repeated chunks or characters.
fn worst_case_inputs(n: usize) -> Vec<(&'static str, String, String)> {
    let x = |n: usize| vec!["x"; n].join("/");
    let a = |n: usize| vec!["a"; n].join("/");
    vec![
        (
            "many_double_wilds",
            "**/x/**/x/**/x/**/y/**/z".into(),
            format!("{}/z", x(n)),
        ),
        ("star_dsl", "$*a$*a$*a$*b$*".into(), "a".repeat(n)),
        ("pico_worst", "**/a/a/a/b/**".into(), format!("{}/b", a(n))),
    ]
}

fn worst_cases(c: &mut Criterion) {
    let mut g = c.benchmark_group("worst_cases");
    // Kept small enough for the backtracking matchers to finish, so that they can be compared.
    for n in [4, 8, 12, 16] {
        for (name, l, r) in worst_case_inputs(n) {
            let (l, r) = (OwnedKeyExpr::new(l).unwrap(), OwnedKeyExpr::new(r).unwrap());
            g.bench_function(format!("{name}/{n}/intersect"), |b| {
                b.iter(|| black_box(&*l).intersects(black_box(&*r)))
            });
            g.bench_function(format!("{name}/{n}/include"), |b| {
                b.iter(|| black_box(&*l).includes(black_box(&*r)))
            });
        }
    }
    g.finish();
}

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2))
}

criterion_group! {
    name = benches;
    config = config();
    targets = test_cases, single_chunks, worst_cases
}
criterion_main!(benches);
