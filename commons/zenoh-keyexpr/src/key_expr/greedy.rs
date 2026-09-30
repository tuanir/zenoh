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

//! Key expression matching without backtracking.
//!
//! Chunks are matched from both ends until a `**` is hit, then verbatim chunks split what's left
//! into independent parts. Between `**`s, each segment is placed at its earliest fit, so nothing
//! is revisited. `$*` inside a chunk works the same way. Worst case is O(n*m) chunk comparisons.
//!
//! Keys must be canonical. An empty slice means no chunks.

use super::{
    include::Includer,
    intersect::{restriction::NoSubWilds, Intersector},
    utils::Split,
    DELIMITER, DOUBLE_WILD, SINGLE_WILD, STAR_DSL,
};

// Step counter for the complexity tests.
#[cfg(test)]
pub(crate) mod steps {
    use core::cell::Cell;
    std::thread_local! {
        static STEPS: Cell<usize> = const { Cell::new(0) };
    }
    pub(crate) fn add(n: usize) {
        STEPS.with(|s| s.set(s.get() + n));
    }
    pub(crate) fn count<T>(f: impl FnOnce() -> T) -> (T, usize) {
        STEPS.with(|s| s.set(0));
        let result = f();
        (result, STEPS.with(|s| s.get()))
    }
}
#[cfg(test)]
use steps::add as step;
#[cfg(not(test))]
#[inline(always)]
fn step(_: usize) {}

const DOUBLE_WILD_PREFIX: &[u8] = b"**/";
const DOUBLE_WILD_SUFFIX: &[u8] = b"/**";
const DOUBLE_WILD_INFIX: &[u8] = b"/**/";

#[inline(always)]
fn first_chunk(s: &[u8]) -> (&[u8], &[u8]) {
    Split::split_once(s, &DELIMITER)
}

#[inline(always)]
fn last_chunk(s: &[u8]) -> (&[u8], &[u8]) {
    let (init, last) = Split::try_rsplit_once(s, &DELIMITER);
    (init.unwrap_or(b""), last)
}

fn no_empty_chunk(s: &[u8]) -> bool {
    s.is_empty() || !s.splitter(&DELIMITER).any(<[u8]>::is_empty)
}

fn is_wrapped_in_double_wild(s: &[u8]) -> bool {
    first_chunk(s).0 == DOUBLE_WILD && last_chunk(s).1 == DOUBLE_WILD
}

fn has_double_wild(s: &[u8]) -> bool {
    s.contains(&SINGLE_WILD) && s.splitter(&DELIMITER).any(|c| c == DOUBLE_WILD)
}

fn is_verbatim(chunk: &[u8]) -> bool {
    chunk.first() == Some(&b'@')
}

fn has_verbatim(s: &[u8]) -> bool {
    s.contains(&b'@') && s.splitter(&DELIMITER).any(is_verbatim)
}

fn next_verbatim(s: &[u8]) -> Option<(&[u8], &[u8], &[u8])> {
    let start = (0..s.len()).find(|&i| s[i] == b'@' && (i == 0 || s[i - 1] == DELIMITER))?;
    let before = s[..start].strip_suffix(&[DELIMITER]).unwrap_or(b"");
    let (chunk, after) = first_chunk(&s[start..]);
    Some((before, chunk, after))
}

// Below this length a naive search is faster than memmem.
const SHORT_TEXT: usize = 8;

/// Whether the `$*`-separated pieces of `pattern` occur in `text` in order, without overlap.
fn pieces_in_order(pattern: &[u8], mut text: &[u8]) -> bool {
    let long = text.len() > SHORT_TEXT;
    for piece in pattern.splitter(STAR_DSL) {
        if piece.is_empty() {
            continue;
        }
        let found = if long {
            memchr::memmem::find(text, piece)
        } else {
            text.windows(piece.len()).position(|w| w == piece)
        };
        let Some(i) = found else {
            step(text.len());
            return false;
        };
        step(i + piece.len());
        text = &text[i + piece.len()..];
    }
    true
}

/// Neither chunk is `**`. Without `DSL`, neither contains `$*`.
fn chunk_intersect<const DSL: bool>(l: &[u8], r: &[u8]) -> bool {
    step(1);
    match (l, r) {
        _ if l == r => true,
        _ if is_verbatim(l) || is_verbatim(r) => false,
        ([SINGLE_WILD], _) | (_, [SINGLE_WILD]) => true,
        _ if DSL && (l.contains(&b'$') || r.contains(&b'$')) => dsl_chunk_intersect(l, r),
        _ => false,
    }
}

type SplitEnd<'a> = fn(&'a [u8]) -> Option<(&'a u8, &'a [u8])>;

fn strip_common<'a>(
    mut l: &'a [u8],
    mut r: &'a [u8],
    stop: u8,
    split: SplitEnd<'a>,
) -> Option<(&'a [u8], &'a [u8])> {
    while let (Some((a, l_rest)), Some((b, r_rest))) = (split(l), split(r)) {
        if *a == stop || *b == stop {
            break;
        }
        step(1);
        if a != b {
            return None;
        }
        (l, r) = (l_rest, r_rest);
    }
    Some((l, r))
}

fn between_stars(s: &[u8]) -> Option<&[u8]> {
    s.strip_prefix(STAR_DSL)?.strip_suffix(STAR_DSL)
}

fn dsl_chunk_intersect(l: &[u8], r: &[u8]) -> bool {
    let Some((l, r)) = strip_common(l, r, b'$', <[u8]>::split_first)
        .and_then(|(l, r)| strip_common(l, r, b'*', <[u8]>::split_last))
    else {
        return false;
    };
    if l.is_empty() || r.is_empty() {
        return matches!(l, b"" | b"$*") && matches!(r, b"" | b"$*");
    }
    // one side starts with `$*`, one ends with `$*`
    match (between_stars(l), between_stars(r)) {
        (Some(m), _) => r.contains(&b'$') || pieces_in_order(m, r),
        (None, Some(m)) => l.contains(&b'$') || pieces_in_order(m, l),
        (None, None) => true,
    }
}

fn chunk_includes(l: &[u8], r: &[u8]) -> bool {
    step(1);
    if l == r {
        return true;
    }
    if is_verbatim(l) || is_verbatim(r) {
        return false;
    }
    if l == [SINGLE_WILD] {
        return true;
    }
    // `l` is `P0$*...$*Pk`. `$*` in `r` is compared as plain text.
    let mut pieces = l.splitter(STAR_DSL);
    let prefix = pieces.next().unwrap_or(b"");
    let Some(suffix) = pieces.next_back() else {
        return false; // `l` has no `$*`
    };
    let middle = l
        .get(prefix.len() + STAR_DSL.len()..l.len() - suffix.len() - STAR_DSL.len())
        .unwrap_or(b"");
    step(prefix.len() + suffix.len());
    let Some(r) = r.strip_prefix(prefix) else {
        return false;
    };
    let Some(r) = r.strip_suffix(suffix) else {
        return false;
    };
    pieces_in_order(middle, r)
}

/// Wildcards never match verbatim chunks, so those must pair up one to one. `segment` checks the
/// parts in between.
fn by_verbatim(mut l: &[u8], mut r: &[u8], segment: impl Fn(&[u8], &[u8]) -> bool) -> bool {
    loop {
        match (next_verbatim(l), next_verbatim(r)) {
            (None, None) => return segment(l, r),
            (Some((l_before, lv, l_after)), Some((r_before, rv, r_after))) => {
                if lv != rv || !segment(l_before, r_before) {
                    return false;
                }
                (l, r) = (l_after, r_after);
            }
            _ => return false,
        }
    }
}

/// Earliest match of `segment` in `y`. Returns the rest of `y`.
fn place<'a>(
    segment: &[u8],
    mut y: &'a [u8],
    matches: &impl Fn(&[u8], &[u8]) -> bool,
) -> Option<&'a [u8]> {
    loop {
        let (mut s, mut t) = (segment, y);
        loop {
            if s.is_empty() {
                return Some(t);
            }
            if t.is_empty() {
                return None;
            }
            let (sc, s_rest) = first_chunk(s);
            let (tc, t_rest) = first_chunk(t);
            if !matches(sc, tc) {
                break;
            }
            (s, t) = (s_rest, t_rest);
        }
        y = first_chunk(y).1;
        if y.is_empty() {
            return None;
        }
    }
}

/// [`place`] for a literal segment, using substring search. `padded` is `/S/`.
fn place_literal<'a>(padded: &[u8], y: &'a [u8]) -> Option<&'a [u8]> {
    let segment = &padded[1..padded.len() - 1];
    step(segment.len());
    if let Some(rest) = y.strip_prefix(segment) {
        match rest.split_first() {
            None => return Some(b""),
            Some((&DELIMITER, rest)) => return Some(rest),
            _ => {}
        }
    }
    let Some(i) = memchr::memmem::find(y, padded) else {
        step(y.len());
        // no trailing `/` at the end of `y`
        return y.ends_with(&padded[..padded.len() - 1]).then_some(b"");
    };
    step(i + padded.len());
    Some(&y[i + padded.len()..])
}

/// `x` is `**/S1/**/.../Sk/**`. The earliest fit of each segment leaves the most room for the
/// next ones, so there's no need to backtrack. With `exact`, literal chunks of `x` only match
/// identical chunks of `y`.
fn greedy(x: &[u8], mut y: &[u8], exact: bool, matches: impl Fn(&[u8], &[u8]) -> bool) -> bool {
    let Some(segments) = x
        .strip_prefix(DOUBLE_WILD_PREFIX)
        .and_then(|x| x.strip_suffix(DOUBLE_WILD_SUFFIX))
    else {
        return true; // `x` is `**/**`
    };
    // offset of `segment` in `x`
    let mut start = DOUBLE_WILD_PREFIX.len();
    for segment in segments.splitter(DOUBLE_WILD_INFIX) {
        let placed = if exact && !segment.contains(&SINGLE_WILD) {
            place_literal(&x[start - 1..start + segment.len() + 1], y)
        } else {
            place(segment, y, &matches)
        };
        match placed {
            Some(after) => y = after,
            None => return false,
        }
        start += segment.len() + DOUBLE_WILD_INFIX.len();
    }
    true
}

/// Strips matching chunks from both ends until a `**` is hit.
#[inline(always)]
fn strip_ends<'a>(
    mut l: &'a [u8],
    mut r: &'a [u8],
    matches: impl Fn(&[u8], &[u8]) -> bool,
) -> Option<(&'a [u8], &'a [u8])> {
    // prefix
    while !l.is_empty() && !r.is_empty() {
        let ((lc, l_rest), (rc, r_rest)) = (first_chunk(l), first_chunk(r));
        if lc == DOUBLE_WILD || rc == DOUBLE_WILD {
            break;
        }
        if !matches(lc, rc) {
            return None;
        }
        (l, r) = (l_rest, r_rest);
    }

    // suffix
    while !l.is_empty() && !r.is_empty() {
        let ((l_init, lc), (r_init, rc)) = (last_chunk(l), last_chunk(r));
        if lc == DOUBLE_WILD || rc == DOUBLE_WILD {
            break;
        }
        if !matches(lc, rc) {
            return None;
        }
        (l, r) = (l_init, r_init);
    }
    Some((l, r))
}

fn segment_intersect<const DSL: bool, const VERBATIM: bool>(l: &[u8], r: &[u8]) -> bool {
    if let (DOUBLE_WILD, x) | (x, DOUBLE_WILD) = (l, r) {
        return !VERBATIM || !has_verbatim(x);
    }
    let Some((l, r)) = strip_ends(l, r, chunk_intersect::<DSL>) else {
        return false;
    };
    match (l, r) {
        (b"" | DOUBLE_WILD, b"" | DOUBLE_WILD) => true,
        (b"", _) | (_, b"") => false,
        _ if VERBATIM && (l.contains(&b'@') || r.contains(&b'@')) => {
            by_verbatim(l, r, segment_intersect::<DSL, false>)
        }
        (DOUBLE_WILD, _) | (_, DOUBLE_WILD) => true,
        _ if is_wrapped_in_double_wild(l) => wrapped_intersect::<DSL>(l, r),
        _ if is_wrapped_in_double_wild(r) => wrapped_intersect::<DSL>(r, l),
        _ => true,
    }
}

/// `x` is `**/M/**`. No verbatim chunks on either side.
fn wrapped_intersect<const DSL: bool>(x: &[u8], y: &[u8]) -> bool {
    // a `**` in `y` can absorb `M`
    let exact = !y.contains(&SINGLE_WILD);
    (!exact && has_double_wild(y)) || greedy(x, y, exact, chunk_intersect::<DSL>)
}

fn segment_includes<const VERBATIM: bool>(l: &[u8], r: &[u8]) -> bool {
    if l == DOUBLE_WILD {
        return !VERBATIM || !has_verbatim(r);
    }
    let Some((l, r)) = strip_ends(l, r, chunk_includes) else {
        return false;
    };
    match (l, r) {
        (b"" | DOUBLE_WILD, b"") => true,
        (b"", _) | (_, b"") => false,
        // only a `**` in `l` can cover a `**` in `r`
        _ if !is_wrapped_in_double_wild(l) => false,
        _ if VERBATIM && (l.contains(&b'@') || r.contains(&b'@')) => {
            by_verbatim(l, r, segment_includes::<false>)
        }
        (DOUBLE_WILD, _) => true,
        _ => greedy(l, r, true, |lc, rc| {
            rc != DOUBLE_WILD && chunk_includes(lc, rc)
        }),
    }
}

pub fn intersect<const DSL: bool>(l: &[u8], r: &[u8]) -> bool {
    debug_assert!(no_empty_chunk(l) && no_empty_chunk(r));
    segment_intersect::<DSL, true>(l, r)
}

pub fn includes(l: &[u8], r: &[u8]) -> bool {
    debug_assert!(no_empty_chunk(l) && no_empty_chunk(r));
    l == r || segment_includes::<true>(l, r)
}

/// [`Intersector`] that never backtracks. O(n*m) in the worst case.
#[derive(Debug)]
pub struct GreedyIntersector;
impl Intersector<NoSubWilds<&[u8]>, NoSubWilds<&[u8]>> for GreedyIntersector {
    fn intersect(&self, left: NoSubWilds<&[u8]>, right: NoSubWilds<&[u8]>) -> bool {
        intersect::<false>(left.0, right.0)
    }
}
impl Intersector<&[u8], &[u8]> for GreedyIntersector {
    fn intersect(&self, left: &[u8], right: &[u8]) -> bool {
        intersect::<true>(left, right)
    }
}

/// [`Includer`] that never backtracks. O(n*m) in the worst case.
#[derive(Debug)]
pub struct GreedyIncluder;
impl Includer<&[u8], &[u8]> for GreedyIncluder {
    fn includes(&self, left: &[u8], right: &[u8]) -> bool {
        includes(left, right)
    }
}
