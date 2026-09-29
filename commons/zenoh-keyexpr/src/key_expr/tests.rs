//
// Copyright (c) 2023 ZettaScale Technology
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

use std::{convert::TryInto, fmt::Debug};

use rand::{rngs::StdRng, Rng, SeedableRng};

use crate::{
    key_expr::{
        fuzzer,
        greedy::steps,
        include::{Includer, LTRIncluder, DEFAULT_INCLUDER},
        intersect::*,
        keyexpr,
    },
    OwnedKeyExpr,
};

type BoxedIntersectors = Vec<Box<dyn for<'a> Intersector<&'a keyexpr, &'a keyexpr> + Send + Sync>>;

lazy_static::lazy_static! {
    // The reference implementations that `DEFAULT_INTERSECTOR` is checked against.
    static ref INTERSECTORS: BoxedIntersectors =
    vec![
        Box::new(ClassicIntersector)
    ];
}

fn intersect<'a, A: TryInto<&'a keyexpr>, B: TryInto<&'a keyexpr>>(l: A, r: B) -> bool
where
    <A as TryInto<&'a keyexpr>>::Error: Debug,
    <B as TryInto<&'a keyexpr>>::Error: Debug,
{
    let left = l.try_into().unwrap();
    let right = r.try_into().unwrap();
    let response = DEFAULT_INTERSECTOR.intersect(left, right);
    for intersector in INTERSECTORS.iter() {
        if intersector.intersect(left, right) != response {
            panic!("DEFAULT_INTERSECTOR ({}) and INTERSECTORS[{:?}] disagreed on intersection between `{}` and `{}`", response, INTERSECTORS.iter().map(|i| i.intersect(left, right)).collect::<Vec<_>>(), left.as_ref(), right.as_ref())
        }
    }
    response
}

#[test]
fn intersections() {
    assert!(intersect("a", "a"));
    assert!(intersect("a/b", "a/b"));
    assert!(intersect("*", "abc"));
    assert!(intersect("*", "xxx"));
    assert!(intersect("ab$*", "abcd"));
    assert!(intersect("ab$*d", "abcd"));
    assert!(intersect("ab$*", "ab"));
    assert!(!intersect("ab/*", "ab"));
    assert!(intersect("a/*/c/*/e", "a/b/c/d/e"));
    assert!(intersect("a/$*b/c/$*d/e", "a/xb/c/xd/e"));
    assert!(!intersect("a/*/c/*/e", "a/c/e"));
    assert!(!intersect("a/*/c/*/e", "a/b/c/d/x/e"));
    assert!(!intersect("ab$*cd", "abxxcxxd"));
    assert!(intersect("ab$*cd", "abxxcxxcd"));
    assert!(!intersect("ab$*cd", "abxxcxxcdx"));
    assert!(intersect("**", "abc"));
    assert!(intersect("**", "a/b/c"));
    assert!(intersect("ab/**", "ab"));
    assert!(intersect("**/xyz", "a/b/xyz/d/e/f/xyz"));
    assert!(!intersect("**/xyz$*xyz", "a/b/xyz/d/e/f/xyz"));
    assert!(intersect("**/xyz$*xyz", "a/b/xyzdefxyz"));
    assert!(intersect("a/**/c/**/e", "a/b/b/b/c/d/d/d/e"));
    assert!(intersect("a/**/c/**/e", "a/c/e"));
    assert!(intersect("a/**/c/*/e/*", "a/b/b/b/c/d/d/c/d/e/f"));
    assert!(!intersect("a/**/c/*/e/*", "a/b/b/b/c/d/d/c/d/d/e/f"));
    assert!(!intersect("ab$*cd", "abxxcxxcdx"));
    assert!(intersect("x/abc", "x/abc"));
    assert!(!intersect("x/abc", "abc"));
    assert!(intersect("x/*", "x/abc"));
    assert!(!intersect("x/*", "abc"));
    assert!(!intersect("*", "x/abc"));
    assert!(intersect("x/*", "x/abc$*"));
    assert!(intersect("x/$*abc", "x/abc$*"));
    assert!(intersect("x/a$*", "x/abc$*"));
    assert!(intersect("x/a$*de", "x/abc$*de"));
    assert!(intersect("x/a$*d$*e", "x/a$*e"));
    assert!(intersect("x/a$*d$*e", "x/a$*c$*e"));
    assert!(intersect("x/a$*d$*e", "x/ade"));
    assert!(!intersect("x/c$*", "x/abc$*"));
    assert!(!intersect("x/$*d", "x/$*e"));

    assert!(intersect("@a", "@a"));
    assert!(!intersect("@a", "@ab"));
    assert!(!intersect("@a", "@a/b"));
    assert!(!intersect("@a", "@a/*"));
    assert!(!intersect("@a", "@a/*/**"));
    assert!(!intersect("@a", "@a$*/**"));
    assert!(intersect("@a", "@a/**"));
    assert!(!intersect("**/xyz$*xyz", "@a/b/xyzdefxyz"));
    assert!(intersect("@a/**/c/**/e", "@a/b/b/b/c/d/d/d/e"));
    assert!(!intersect("@a/**/c/**/e", "@a/@b/b/b/c/d/d/d/e"));
    assert!(intersect("@a/**/@c/**/e", "@a/b/b/b/@c/d/d/d/e"));
    assert!(intersect("@a/**/e", "@a/b/b/d/d/d/e"));
    assert!(intersect("@a/**/e", "@a/b/b/b/d/d/d/e"));
    assert!(intersect("@a/**/e", "@a/b/b/c/d/d/d/e"));
    assert!(!intersect("@a/**/e", "@a/b/b/@c/b/d/d/d/e"));
    assert!(!intersect("@a/*", "@a/@b"));
    assert!(!intersect("@a/**", "@a/@b"));
    assert!(intersect("@a/**/@b", "@a/@b"));
    assert!(intersect("@a/@b/**", "@a/@b"));
    assert!(intersect("@a/**/@c/**/@b", "@a/**/@c/@b"));
    assert!(intersect("@a/**/@c/**/@b", "@a/@c/**/@b"));
    assert!(intersect("@a/**/@c/@b", "@a/@c/**/@b"));
    assert!(!intersect("@a/**/@b", "@a/**/@c/**/@b"));
    assert!(intersect("@a", "**/@a"));
}

fn includes<
    'a,
    A: TryInto<&'a keyexpr, Error = zenoh_result::Error>,
    B: TryInto<&'a keyexpr, Error = zenoh_result::Error>,
>(
    l: A,
    r: B,
) -> bool {
    let left = l.try_into().unwrap();
    let right = r.try_into().unwrap();
    let response = left.includes(right);
    let reference = LTRIncluder.includes(left, right);
    if response != reference {
        panic!("DEFAULT_INCLUDER ({response}) and LTRIncluder ({reference}) disagreed on whether `{left}` includes `{right}`")
    }
    response
}

#[test]
fn inclusions() {
    assert!(includes("a", "a"));
    assert!(includes("a/b", "a/b"));
    assert!(includes("*", "abc"));
    assert!(includes("*", "xxx"));
    assert!(includes("ab$*", "abcd"));
    assert!(includes("ab$*d", "abcd"));
    assert!(includes("ab$*", "ab"));
    assert!(!includes("ab/*", "ab"));
    assert!(includes("a/*/c/*/e", "a/b/c/d/e"));
    assert!(includes("a/$*b/c/$*d/e", "a/xb/c/xd/e"));
    assert!(!includes("a/*/c/*/e", "a/c/e"));
    assert!(!includes("a/*/c/*/e", "a/b/c/d/x/e"));
    assert!(!includes("ab$*cd", "abxxcxxd"));
    assert!(includes("ab$*c$*d", "abxxcxxd"));
    assert!(includes("ab$*cd", "abxxcxxcd"));
    assert!(!includes("ab$*cd", "abxxcxxcdx"));
    assert!(includes("**", "abc"));
    assert!(includes("**", "a/b/c"));
    assert!(includes("ab/**", "ab"));
    assert!(includes("**/xyz", "a/b/xyz/d/e/f/xyz"));
    assert!(!includes("**/xyz$*xyz", "a/b/xyz/d/e/f/xyz"));
    assert!(includes("**/xyz$*xyz", "a/b/xyzdefxyz"));
    assert!(includes("a/**/c/**/e", "a/b/b/b/c/d/d/d/e"));
    assert!(includes("a/**/c/**/e", "a/c/e"));
    assert!(includes("a/**/c/*/e/*", "a/b/b/b/c/d/d/c/d/e/f"));
    assert!(!includes("a/**/c/*/e/*", "a/b/b/b/c/d/d/c/d/d/e/f"));
    assert!(!includes("ab$*cd", "abxxcxxcdx"));
    assert!(includes("x/abc", "x/abc"));
    assert!(!includes("x/abc", "abc"));
    assert!(includes("x/*", "x/abc"));
    assert!(!includes("x/*", "abc"));
    assert!(!includes("*", "x/abc"));
    assert!(includes("x/*", "x/abc$*"));
    assert!(!includes("x/$*abc", "x/abc$*"));
    assert!(includes("x/a$*", "x/abc$*"));
    assert!(!includes("x/abc$*", "x/a$*"));
    assert!(includes("x/a$*de", "x/abc$*de"));
    assert!(includes("x/a$*e", "x/a$*d$*e"));
    assert!(!includes("x/a$*d$*e", "x/a$*e"));
    assert!(!includes("x/a$*d$*e", "x/a$*c$*e"));
    assert!(includes("x/a$*d$*e", "x/ade"));
    assert!(!includes("x/c$*", "x/abc$*"));
    assert!(includes("x/$*c$*", "x/abc$*"));
    assert!(!includes("x/$*d", "x/$*e"));

    assert!(includes("@a", "@a"));
    assert!(!includes("@a", "@ab"));
    assert!(!includes("@a", "@a/b"));
    assert!(!includes("@a", "@a/*"));
    assert!(!includes("@a", "@a/*/**"));
    assert!(!includes("@a$*/**", "@a"));
    assert!(!includes("@a", "@a/**"));
    assert!(includes("@a/**", "@a"));
    assert!(!includes("**/xyz$*xyz", "@a/b/xyzdefxyz"));
    assert!(includes("@a/**/c/**/e", "@a/b/b/b/c/d/d/d/e"));
    assert!(!includes("@a/*", "@a/@b"));
    assert!(!includes("@a/**", "@a/@b"));
    assert!(includes("@a/**/@b", "@a/@b"));
    assert!(includes("@a/@b/**", "@a/@b"));
}

/// Checks against the old matchers, and the properties that hold for any pair.
fn check_pair(a: &keyexpr, b: &keyexpr) {
    let intersects = DEFAULT_INTERSECTOR.intersect(a, b);
    assert_eq!(
        intersects,
        ClassicIntersector.intersect(a, b),
        "intersect(`{a}`, `{b}`) disagrees with ClassicIntersector"
    );
    assert_eq!(
        intersects,
        DEFAULT_INTERSECTOR.intersect(b, a),
        "intersect(`{a}`, `{b}`) is not symmetric"
    );
    for (l, r) in [(a, b), (b, a)] {
        let includes = DEFAULT_INCLUDER.includes(l, r);
        assert_eq!(
            includes,
            LTRIncluder.includes(l, r),
            "includes(`{l}`, `{r}`) disagrees with LTRIncluder"
        );
        assert!(
            !includes || intersects,
            "`{l}` includes `{r}` but they don't intersect"
        );
    }
    assert!(
        DEFAULT_INCLUDER.includes(a, a),
        "`{a}` doesn't include itself"
    );
}

// Also produces `@` chunks with `$*`, and plain chunks with an `@` inside.
fn random_chunk(rng: &mut impl Rng, alphabet: &[u8]) -> String {
    let letter = |rng: &mut dyn rand::RngCore| alphabet[rng.gen_range(0..alphabet.len())] as char;
    let t: f64 = rng.gen();
    if t < 0.15 {
        return "**".into();
    }
    if t < 0.30 {
        return "*".into();
    }
    if t < 0.40 {
        let mut s: String = "@".into();
        for _ in 0..rng.gen_range(1..=2) {
            s.push(letter(rng));
        }
        if rng.gen_bool(0.2) {
            s.push_str("$*");
        }
        return s;
    }
    let mut s = String::new();
    for _ in 0..rng.gen_range(1..=4) {
        if rng.gen_bool(0.35) && !s.ends_with("$*") {
            s.push_str("$*");
        } else {
            s.push(letter(rng));
        }
    }
    if s == "$*" {
        s = letter(rng).into();
    }
    if rng.gen_bool(0.03) {
        s.push('@');
        s.push(letter(rng));
    }
    s
}

fn canon_key(chunks: Vec<String>) -> OwnedKeyExpr {
    let key = chunks.join("/");
    OwnedKeyExpr::autocanonize(key.clone())
        .unwrap_or_else(|e| panic!("generated an invalid key expression `{key}`: {e}"))
}

fn random_key(rng: &mut impl Rng, alphabet: &[u8]) -> OwnedKeyExpr {
    let n = rng.gen_range(1..=7);
    canon_key((0..n).map(|_| random_chunk(rng, alphabet)).collect())
}

// `key` with some chunks replaced, so the pair likely matches.
fn related_key(rng: &mut impl Rng, alphabet: &[u8], key: &keyexpr) -> OwnedKeyExpr {
    canon_key(
        key.as_str()
            .split('/')
            .map(|c| {
                if rng.gen_bool(0.7) {
                    c.to_owned()
                } else {
                    random_chunk(rng, alphabet)
                }
            })
            .collect(),
    )
}

fn fuzz_pairs(seed: u64, rounds: usize, alphabet: &[u8]) {
    let mut rng = StdRng::seed_from_u64(seed);
    let (mut intersecting, mut including) = (0, 0);
    for _ in 0..rounds {
        let a = random_key(&mut rng, alphabet);
        let b = if rng.gen_bool(0.3) {
            related_key(&mut rng, alphabet, &a)
        } else {
            random_key(&mut rng, alphabet)
        };
        check_pair(&a, &b);
        intersecting += a.intersects(&b) as usize;
        including += a.includes(&b) as usize;
    }
    // make sure enough pairs actually match
    assert!(
        intersecting > rounds / 20,
        "only {intersecting} of {rounds} pairs intersect"
    );
    assert!(
        including > rounds / 50,
        "only {including} of {rounds} pairs include"
    );
}

#[test]
fn fuzz() {
    const FUZZ_ROUNDS: usize = 100_000;
    let rng = rand::thread_rng();
    let mut fuzzer = fuzzer::KeyExprFuzzer(rng);
    let mut ke1 = fuzzer.next().unwrap();
    for ke2 in fuzzer.take(FUZZ_ROUNDS) {
        check_pair(&ke1, &ke2);
        ke1 = ke2;
    }
    fuzz_pairs(1, FUZZ_ROUNDS, b"ab");
    fuzz_pairs(2, FUZZ_ROUNDS, b"a");
}

/// A longer run: `cargo test --release -p zenoh-keyexpr -- --ignored fuzz_long`.
#[test]
#[ignore]
fn fuzz_long() {
    const FUZZ_ROUNDS: usize = 1_000_000;
    let mut fuzzer = fuzzer::KeyExprFuzzer(StdRng::seed_from_u64(3));
    let mut ke1 = fuzzer.next().unwrap();
    for ke2 in fuzzer.take(FUZZ_ROUNDS) {
        check_pair(&ke1, &ke2);
        ke1 = ke2;
    }
    fuzz_pairs(4, FUZZ_ROUNDS, b"ab");
    fuzz_pairs(5, FUZZ_ROUNDS, b"a");
    fuzz_pairs(6, FUZZ_ROUNDS, b"abc");
}

#[test]
fn edge_cases() {
    let x = |n: usize| vec!["x"; n].join("/");
    let long = [
        (format!("**/{}/**", x(64)), x(65), true, true, false),
        (format!("**/{}/**", x(65)), x(64), false, false, false),
        (
            format!("**/{}/y/**", x(64)),
            format!("{}/y", x(65)),
            true,
            true,
            false,
        ),
        (format!("**/{}/**", x(65)), x(65), true, true, false),
    ];
    // (a, b, intersects(a, b), includes(a, b), includes(b, a))
    let cases = [
        // zenoh-pico disagrees on these
        ("a/**", "a/x@y", true, true, false),
        ("@a", "@a$*/**", false, false, false),
        ("@a$*/**", "@a", false, false, false),
        // ** on both sides
        ("**/a/**", "**/b/**", true, false, false),
        ("a/**/b", "a/**/c", false, false, false),
        ("**", "**", true, true, true),
        ("*/**", "**", true, false, true),
        // $* on both sides
        ("a$*", "$*b", true, false, false),
        ("a$*b", "a$*c", false, false, false),
        ("$*a$*", "b$*c", true, false, false),
        ("$*ab$*", "a$*b", true, false, false),
        ("a$*b$*c", "a$*c", true, false, true),
        // leading and trailing **, and the shortcuts
        ("**/a", "b/**", true, false, false),
        ("**/a", "b/**/c", false, false, false),
        ("a/**", "**/b", true, false, false),
        ("**/a", "a/**", true, false, false),
        ("**/a/**/b", "b/**/a", false, false, false),
        ("**/a/**", "b", false, false, false),
        ("**/a/**", "x/b/**/c", true, false, false),
        // verbatim chunks
        ("**/@a/**", "@a", true, true, false),
        ("**", "@a", false, false, false),
        ("a/@b/**", "a/@b/c/@d", false, false, false),
    ];
    let cases = cases
        .iter()
        .map(|&(a, b, i, ab, ba)| (a.to_owned(), b.to_owned(), i, ab, ba))
        .chain(long);
    for (a, b, i, ab, ba) in cases {
        assert_eq!(
            intersect(a.as_str(), b.as_str()),
            i,
            "intersect(`{a}`, `{b}`)"
        );
        assert_eq!(
            intersect(b.as_str(), a.as_str()),
            i,
            "intersect(`{b}`, `{a}`)"
        );
        assert_eq!(
            includes(a.as_str(), b.as_str()),
            ab,
            "includes(`{a}`, `{b}`)"
        );
        assert_eq!(
            includes(b.as_str(), a.as_str()),
            ba,
            "includes(`{b}`, `{a}`)"
        );
    }
}

// Checked key expressions can't have a lone `$`, but the byte-level traits take any bytes.
// The old matchers panic on some of these, so the expected results are listed here.
#[test]
fn lone_dollar() {
    use crate::key_expr::{greedy::GreedyIntersector, include::GreedyIncluder};
    // (a, b, intersects(a, b), includes(a, b)); `b` never includes `a`.
    let cases: [(&str, &str, bool, bool); 6] = [
        ("a$", "a", false, false),
        ("$", "x", false, false),
        ("a$*b$", "ab$", true, true),
        ("$a$*", "$ab", true, true),
        ("a$*$", "a$", true, true),
        ("$*a$", "xa$", true, true),
    ];
    for (a, b, i, ab) in cases {
        let (a_bytes, b_bytes) = (a.as_bytes(), b.as_bytes());
        assert_eq!(
            GreedyIntersector.intersect(a_bytes, b_bytes),
            i,
            "intersect(`{a}`, `{b}`)"
        );
        assert_eq!(
            GreedyIntersector.intersect(b_bytes, a_bytes),
            i,
            "intersect(`{b}`, `{a}`)"
        );
        assert_eq!(
            GreedyIncluder.includes(a_bytes, b_bytes),
            ab,
            "includes(`{a}`, `{b}`)"
        );
        assert!(
            !GreedyIncluder.includes(b_bytes, a_bytes),
            "includes(`{b}`, `{a}`)"
        );
    }
}

fn max_steps(a: &str, b: &str) -> usize {
    let (a, b) = (keyexpr::new(a).unwrap(), keyexpr::new(b).unwrap());
    [(a, b), (b, a)]
        .into_iter()
        .flat_map(|(l, r)| {
            [
                steps::count(|| l.intersects(r)).1,
                steps::count(|| l.includes(r)).1,
            ]
        })
        .max()
        .unwrap()
}

// Inputs that blow up backtracking or naive matchers.
#[test]
fn linear_complexity() {
    let x = |n: usize| vec!["x"; n].join("/");
    let cases = [
        (
            "**/x/**/x/**/x/**/y/**/z".to_owned(),
            format!("{}/z", x(1000)),
        ),
        (
            "**/x/**/x/**/x/**/x/**/y/**/z".to_owned(),
            format!("{}/z", x(1000)),
        ),
        ("$*aaab$*".to_owned(), format!("{}b", "a".repeat(5000))),
        ("$*a$*a$*a$*b".to_owned(), "a".repeat(5000)),
        ("$*a$*a$*a$*b$*".to_owned(), "a".repeat(5000)),
        (
            "$*a$*a$*a$*b$*".to_owned(),
            format!("{}$*", "a".repeat(5000)),
        ),
        ("a/b/**".to_owned(), format!("a/b/{}", x(1000))),
        ("**/a".to_owned(), format!("{}/a", x(1000))),
        (format!("a/{}/**", x(500)), format!("**/{}/b", x(500))),
        (
            format!("**/{}/**", x(500)),
            format!("{}/**/{}", x(500), x(500)),
        ),
    ];
    for (a, b) in &cases {
        let steps = max_steps(a, b);
        let bound = 4 * (a.len() + b.len());
        assert!(
            steps <= bound,
            "`{a:.40}` vs `{b:.40}`: {steps} steps, bound {bound}"
        );
    }
}

// The search between `**`s can be quadratic, but no worse.
#[test]
fn quadratic_complexity() {
    let a = |n: usize| vec!["a"; n].join("/");
    let stars = |n: usize| vec!["*"; n].join("/");
    let cases = [
        // zenoh-pico's worst case
        ("**/a/a/a/b/**".to_owned(), format!("{}/b", a(2000))),
        // a segment of more than 64 chunks, with `*` chunks
        (format!("**/a/{}/b/**", stars(70)), a(1000)),
        (format!("**/a/{}/b/**", stars(70)), format!("{}/b", a(1000))),
    ];
    for (l, r) in &cases {
        let steps = max_steps(l, r);
        let bound = 2 * l.len() * r.len();
        assert!(
            steps <= bound,
            "`{l:.40}` vs `{r:.40}`: {steps} steps, bound {bound}"
        );
    }
}
