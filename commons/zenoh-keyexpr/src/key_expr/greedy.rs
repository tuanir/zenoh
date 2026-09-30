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

pub fn intersect<const DSL: bool>(l: &[u8], r: &[u8]) -> bool {
    debug_assert!(l.no_empty_chunk() && r.no_empty_chunk());
    segment_intersect::<DSL, true>(l, r)
}

pub fn includes(l: &[u8], r: &[u8]) -> bool {
    debug_assert!(l.no_empty_chunk() && r.no_empty_chunk());
    segment_includes::<true>(l, r)
}

fn segment_intersect<const DSL: bool, const VERBATIM: bool>(l: &[u8], r: &[u8]) -> bool {
    if let (DOUBLE_WILD, x) | (x, DOUBLE_WILD) = (l, r) {
        return !VERBATIM || !x.has_verbatim();
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
        _ if l.is_wrapped_in_double_wild() => wrapped_intersect::<DSL>(l, r),
        _ if r.is_wrapped_in_double_wild() => wrapped_intersect::<DSL>(r, l),
        _ => true,
    }
}

/// `x` is `**/M/**`. No verbatim chunks on either side.
fn wrapped_intersect<const DSL: bool>(x: &[u8], y: &[u8]) -> bool {
    let exact = !y.contains(&SINGLE_WILD);
    // a `**` in `y` can absorb `M`
    (!exact && y.has_double_wild()) || greedy(x, y, exact, chunk_intersect::<DSL>)
}

fn segment_includes<const VERBATIM: bool>(l: &[u8], r: &[u8]) -> bool {
    if l == DOUBLE_WILD {
        return !VERBATIM || !r.has_verbatim();
    }
    let Some((l, r)) = strip_ends(l, r, chunk_includes) else {
        return false;
    };
    match (l, r) {
        (b"" | DOUBLE_WILD, b"") => true,
        (b"", _) | (_, b"") => false,
        // only a `**` in `l` can cover a `**` in `r`
        _ if !l.is_wrapped_in_double_wild() => false,
        _ if VERBATIM && (l.contains(&b'@') || r.contains(&b'@')) => {
            by_verbatim(l, r, segment_includes::<false>)
        }
        (DOUBLE_WILD, _) => true,
        _ => greedy(l, r, true, |lc, rc| {
            rc != DOUBLE_WILD && chunk_includes(lc, rc)
        }),
    }
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
        let ((lc, l_rest), (rc, r_rest)) = (l.first_chunk_and_rest(), r.first_chunk_and_rest());
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
        let ((l_init, lc), (r_init, rc)) = (l.rest_and_last_chunk(), r.rest_and_last_chunk());
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

/// Wildcards never match verbatim chunks, so those must pair up one to one. `segment` checks the
/// parts in between.
fn by_verbatim(mut l: &[u8], mut r: &[u8], segment: impl Fn(&[u8], &[u8]) -> bool) -> bool {
    loop {
        match (l.next_verbatim(), r.next_verbatim()) {
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

/// `x` is `**/S1/**/.../Sk/**`. The earliest fit of each segment leaves the most room for the
/// next ones, so there's no need to backtrack. Set `exact` only if literal chunks of `x` can match
/// nothing but identical chunks of `y`; that allows a substring search.
fn greedy(x: &[u8], mut y: &[u8], exact: bool, matches: impl Fn(&[u8], &[u8]) -> bool) -> bool {
    let Some(segments) = x
        .strip_prefix(DOUBLE_WILD_PREFIX)
        .and_then(|x| x.strip_suffix(DOUBLE_WILD_SUFFIX))
    else {
        return true; // `x` is `**`
    };
    // offset of `segment` in `x`
    let mut start = DOUBLE_WILD_PREFIX.len();
    for segment in segments.splitter(DOUBLE_WILD_INFIX) {
        let placed = if exact && !segment.contains(&SINGLE_WILD) {
            find_literal_segment(&x[start - 1..start + segment.len() + 1], y)
        } else {
            find_segment(segment, y, &matches)
        };
        match placed {
            Some(after) => y = after,
            None => return false,
        }
        start += segment.len() + DOUBLE_WILD_INFIX.len();
    }
    true
}

/// Earliest match of `segment` in `y`. Returns the rest of `y`.
fn find_segment<'a>(
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
            let (sc, s_rest) = s.first_chunk_and_rest();
            let (tc, t_rest) = t.first_chunk_and_rest();
            if !matches(sc, tc) {
                break;
            }
            (s, t) = (s_rest, t_rest);
        }
        y = y.first_chunk_and_rest().1;
        if y.is_empty() {
            return None;
        }
    }
}

/// [`find_segment`] for a literal segment, using substring search. `padded` is `/S/`.
fn find_literal_segment<'a>(padded: &[u8], y: &'a [u8]) -> Option<&'a [u8]> {
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

/// Neither chunk is `**`. Without `DSL`, neither contains `$*`.
fn chunk_intersect<const DSL: bool>(l: &[u8], r: &[u8]) -> bool {
    step(1);
    match (l, r) {
        _ if l == r => true,
        _ if l.is_verbatim() || r.is_verbatim() => false,
        ([SINGLE_WILD], _) | (_, [SINGLE_WILD]) => true,
        _ if DSL && (l.contains(&b'$') || r.contains(&b'$')) => dsl_chunk_intersect(l, r),
        _ => false,
    }
}

fn dsl_chunk_intersect(l: &[u8], r: &[u8]) -> bool {
    let Some((l, r)) = strip_common_bytes(l, r, b'$', <[u8]>::split_first)
        .and_then(|(l, r)| strip_common_bytes(l, r, b'*', <[u8]>::split_last))
    else {
        return false;
    };
    if l.is_empty() || r.is_empty() {
        return matches!(l, b"" | b"$*") && matches!(r, b"" | b"$*");
    }
    // one side starts with `$*`, one ends with `$*`
    match (l.strip_outer_stars(), r.strip_outer_stars()) {
        (Some(m), _) => r.contains(&b'$') || pieces_in_order(m, r),
        (None, Some(m)) => l.contains(&b'$') || pieces_in_order(m, l),
        (None, None) => true,
    }
}

type SplitEnd<'a> = fn(&'a [u8]) -> Option<(&'a u8, &'a [u8])>;

fn strip_common_bytes<'a>(
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

fn chunk_includes(l: &[u8], r: &[u8]) -> bool {
    step(1);
    if l == r {
        return true;
    }
    if l.is_verbatim() || r.is_verbatim() {
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

// Up to this length a naive search is faster than memmem.
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

trait KeyBytes {
    fn first_chunk_and_rest(&self) -> (&Self, &Self);
    fn rest_and_last_chunk(&self) -> (&Self, &Self);
    fn no_empty_chunk(&self) -> bool;
    fn is_wrapped_in_double_wild(&self) -> bool;
    fn has_double_wild(&self) -> bool;
    fn is_verbatim(&self) -> bool;
    fn has_verbatim(&self) -> bool;
    /// `(before, chunk, after)`
    fn next_verbatim(&self) -> Option<(&Self, &Self, &Self)>;
    /// `$*M$*` -> `M`
    fn strip_outer_stars(&self) -> Option<&Self>;
}

impl KeyBytes for [u8] {
    #[inline(always)]
    fn first_chunk_and_rest(&self) -> (&[u8], &[u8]) {
        Split::split_once(self, &DELIMITER)
    }

    #[inline(always)]
    fn rest_and_last_chunk(&self) -> (&[u8], &[u8]) {
        let (init, last) = Split::try_rsplit_once(self, &DELIMITER);
        (init.unwrap_or(b""), last)
    }

    fn no_empty_chunk(&self) -> bool {
        self.is_empty() || !self.splitter(&DELIMITER).any(<[u8]>::is_empty)
    }

    fn is_wrapped_in_double_wild(&self) -> bool {
        self.first_chunk_and_rest().0 == DOUBLE_WILD && self.rest_and_last_chunk().1 == DOUBLE_WILD
    }

    fn has_double_wild(&self) -> bool {
        self.contains(&SINGLE_WILD) && self.splitter(&DELIMITER).any(|c| c == DOUBLE_WILD)
    }

    fn is_verbatim(&self) -> bool {
        self.first() == Some(&b'@')
    }

    fn has_verbatim(&self) -> bool {
        self.contains(&b'@') && self.splitter(&DELIMITER).any(KeyBytes::is_verbatim)
    }

    fn next_verbatim(&self) -> Option<(&[u8], &[u8], &[u8])> {
        let start =
            (0..self.len()).find(|&i| self[i] == b'@' && (i == 0 || self[i - 1] == DELIMITER))?;
        let before = self[..start].strip_suffix(&[DELIMITER]).unwrap_or(b"");
        let (chunk, after) = self[start..].first_chunk_and_rest();
        Some((before, chunk, after))
    }

    fn strip_outer_stars(&self) -> Option<&[u8]> {
        self.strip_prefix(STAR_DSL)?.strip_suffix(STAR_DSL)
    }
}

const DOUBLE_WILD_PREFIX: &[u8] = b"**/";
const DOUBLE_WILD_SUFFIX: &[u8] = b"/**";
const DOUBLE_WILD_INFIX: &[u8] = b"/**/";

#[cfg(test)]
use steps::add as step;
#[cfg(not(test))]
#[inline(always)]
fn step(_: usize) {}

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
