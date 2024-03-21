// Copyright (c) 2024 Trung Tran <tqtrungse@gmail.com>
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

use std::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicU32, Ordering},
};

/// Element for bounded queue.
pub(crate) struct ElemArr<T> {
    lap: AtomicU32,
    msg: UnsafeCell<MaybeUninit<T>>,
}

impl<T> Default for ElemArr<T> {
    #[inline]
    fn default() -> Self {
        Self {
            lap: AtomicU32::new(0),
            msg: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
}

/// Element for unbounded queue.
impl<T> ElemArr<T> {
    #[inline]
    pub(crate) fn load_lap(&self) -> u32 {
        self.lap.load(Ordering::Acquire)
    }

    #[inline]
    pub(crate) fn write(&self, lap: u32, msg: T) {
        unsafe { self.msg.get().write(MaybeUninit::new(msg)); }
        // Make the element available for reading.
        self.lap.store(lap, Ordering::Release);
    }

    #[inline]
    pub(crate) fn read(&self, lap: u32) -> T {
        // We own the element, do non-atomic read and remove.
        let msg: T;
        unsafe { msg = self.msg.get().read().assume_init(); }

        // Make the element available for writing.
        self.lap.store(lap, Ordering::Release);
        msg
    }

    #[inline]
    pub(crate) fn atom_lap(&self) -> *const AtomicU32 {
        &self.lap
    }
}

// pub(crate) struct ElemList<T> {
//     size: usize,
//     msg: UnsafeCell<MaybeUninit<T>>,
//     next: AtomicPtr<ElemList<T>>,
// }
//
// impl<T> Default for ElemList<T> {
//     #[inline]
//     fn default() -> Self {
//         let mut elem = ElemList {
//             size: 0,
//             msg: UnsafeCell::new(MaybeUninit::uninit()),
//             next: AtomicPtr::new(null_mut()),
//         };
//         elem.size = std::mem::size_of_val(&elem);
//         elem
//     }
// }
//
// impl<T> ElemList<T> {
//     #[inline]
//     pub(crate) fn new(msg: T) -> Self {
//         let mut elem = ElemList {
//             size: 0,
//             msg: UnsafeCell::new(MaybeUninit::new(msg)),
//             next: AtomicPtr::new(null_mut()),
//         };
//         elem.size = std::mem::size_of_val(&elem);
//         elem
//     }
//
//     #[inline]
//     pub(crate) fn get_next(&self) -> *mut ElemList<T> {
//         self.next.load(Ordering::Relaxed)
//     }
//
//     #[inline]
//     pub(crate) fn set_next(&self, elem: *mut ElemList<T>) {
//         self.next.store(elem, Ordering::Relaxed);
//     }
//
//     #[inline]
//     pub(crate) fn move_value(&mut self) -> T {
//         unsafe { self.msg.get().read().assume_init() }
//     }
//
//     #[inline]
//     pub(crate) fn size(&self) -> usize {
//         self.size
//     }
// }