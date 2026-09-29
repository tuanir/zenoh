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
use super::keyexpr;

pub use super::greedy::GreedyIncluder;

pub const DEFAULT_INCLUDER: GreedyIncluder = GreedyIncluder;

pub trait Includer<Left, Right> {
    /// Returns `true` if the set defined by `left` includes the one defined by `right`
    fn includes(&self, left: Left, right: Right) -> bool;
}

impl<T: for<'a> Includer<&'a [u8], &'a [u8]>> Includer<&keyexpr, &keyexpr> for T {
    fn includes(&self, left: &keyexpr, right: &keyexpr) -> bool {
        let left = left.as_bytes();
        let right = right.as_bytes();
        if left == right {
            return true;
        }
        self.includes(left, right)
    }
}
