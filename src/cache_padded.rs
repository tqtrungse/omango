// Copyright (c) 2022 Trung <tqtrungse@gmail.com>
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! This file was re-modified based on the following program in Crossbeam-Utils.
//!
//! Source:
//!   - `<https://github.com/crossbeam-rs/crossbeam/blob/master/crossbeam-utils/src/cache_padded.rs>`
//!
//! Copyright & License:
//!   - The Crossbeam Project Developers
//!   - The MIT License (MIT) or Apache License 2.0
//!   - '<https://opensource.org/licenses/MIT>'
//!   - '<https://www.apache.org/licenses/LICENSE-2.0>'

use core::ops::{Deref, DerefMut};

#[derive(Clone, Copy, Default, Hash, PartialEq, Eq)]
#[cfg_attr(
any(
target_arch = "x86_64",
target_arch = "aarch64",
target_arch = "powerpc64",
),
repr(align(128))
)]
#[cfg_attr(
any(
target_arch = "arm",
target_arch = "mips",
target_arch = "mips64",
target_arch = "riscv64",
),
repr(align(32))
)]
#[cfg_attr(target_arch = "s390x", repr(align(256)))]
#[cfg_attr(
not(any(
target_arch = "x86_64",
target_arch = "aarch64",
target_arch = "powerpc64",
target_arch = "arm",
target_arch = "mips",
target_arch = "mips64",
target_arch = "riscv64",
target_arch = "s390x",
)),
repr(align(64))
)]
/// Aligns an object size to CPU cache line size to prevent false-sharing.
pub(crate) struct CachePadded<T> {
    value: T,
}

unsafe impl<T: Send> Send for CachePadded<T> {}

unsafe impl<T: Sync> Sync for CachePadded<T> {}

impl<T> CachePadded<T> {
    /// Create an object is aligned to CPU cache line size.
    #[inline]
    pub(crate) const fn new(t: T) -> CachePadded<T> {
        CachePadded::<T> { value: t }
    }
}

impl<T> Deref for CachePadded<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for CachePadded<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}