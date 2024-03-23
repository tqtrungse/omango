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
    ptr::null_mut,
    sync::atomic::{AtomicPtr, AtomicBool, Ordering},
};

use omango_util::{
    backoff::Backoff,
    cache_padded::CachePadded,
    hint::{likely, unlikely},
};

use crate::{
    queue::{
        bounded::{Bounded, SpscBounded, MpmcBounded},
        error::{RecvError, SendError, TrySendError},
    },
};

const SUB_QUEUE_SIZE: u32 = 1024;

macro_rules! set_close {
    ($tail:expr, $closed:expr, $next:expr) => {
        loop {
            let tail = $tail.load(Ordering::Relaxed);
            match unsafe {
                (*tail).next.compare_exchange_weak(
                    null_mut(),
                    $next,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
            } {
                Ok(_) => {
                    $closed.store(true, Ordering::Relaxed);
                    break;
                }
                Err(_) => {}
            }
        }
    };
}

macro_rules! close_current_node {
    ($head:expr) => {
        loop {
            unsafe {
                (*$head).queue.close();
                let next = (*$head).next.load(Ordering::Relaxed);
                if !next.is_null() {
                    (*next).queue.close();
                    return;
                } else {
                    $head = next;
                }
            }
        }
    };
}

struct SpscNode<T> {
    queue: SpscBounded<T>,
    next: AtomicPtr<SpscNode<T>>,
}

impl<T> SpscNode<T> {
    #[inline]
    fn new(cap: u32) -> Self {
        Self {
            queue: SpscBounded::new(cap),
            next: AtomicPtr::new(null_mut()),
        }
    }
}

struct Pointer<T> {
    ptr: *mut SpscNode<T>,
}

impl<T> Pointer<T> {
    #[inline]
    fn new(ptr: *mut SpscNode<T>) -> Self {
        Self {
            ptr,
        }
    }

    #[inline]
    fn get(&self) -> *mut SpscNode<T> {
        self.ptr
    }

    #[inline]
    fn set(&mut self, ptr: *mut SpscNode<T>) {
        self.ptr = ptr;
    }
}

pub(crate) struct SpscUnbounded<T> {
    head: CachePadded<Pointer<T>>,
    tail: CachePadded<AtomicPtr<SpscNode<T>>>,

    closed: CachePadded<AtomicBool>,
}

impl<T> Default for SpscUnbounded<T> {
    #[inline]
    fn default() -> Self {
        let init_node = Box::into_raw(Box::new(SpscNode::new(SUB_QUEUE_SIZE)));
        Self {
            head: CachePadded::new(Pointer::new(init_node)),
            tail: CachePadded::new(AtomicPtr::new(init_node)),
            closed: CachePadded::new(AtomicBool::new(false)),
        }
    }
}

impl<T> SpscUnbounded<T> {
    #[inline]
    pub(crate) fn send(&mut self, msg: T) -> Result<(), SendError<T>> {
        let tail = self.tail.load(Ordering::Relaxed);
        let result = unsafe { (*tail).queue.try_send(msg) };

        return match result {
            Ok(()) => {
                Ok(())
            }
            Err(TrySendError::Disconnected(msg)) => {
                Err(SendError(msg))
            }
            Err(_) => {
                unsafe { (*tail).queue.close() };

                let next: *mut SpscNode<T> = Box::into_raw(Box::new(SpscNode::new(SUB_QUEUE_SIZE)));
                match unsafe {
                    (*tail).next.compare_exchange_weak(
                        null_mut(),
                        next,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    )
                } {
                    Ok(_) => {
                        let _ = unsafe { (*next).queue.try_send(result.err().unwrap().into_inner()) };
                        self.tail.store(next, Ordering::Relaxed);
                        Ok(())
                    }
                    Err(v) => {
                        let _ = unsafe { Box::from_raw(next) };
                        self.tail.store(v, Ordering::Relaxed);
                        Err(SendError(result.err().unwrap().into_inner()))
                    }
                }
            }
        };
    }

    pub(crate) fn recv(&mut self) -> Result<T, RecvError> {
        let head = self.head.get();
        let result = unsafe { (*head).queue.recv((*head).queue.cast()) };
        if likely(result.is_ok()) {
            return Ok(result.unwrap());
        }

        if unlikely(self.closed.load(Ordering::Relaxed)) {
            return Err(RecvError);
        }

        let backoff = Backoff::default();
        let mut next = unsafe { (*head).next.load(Ordering::Relaxed) };
        while next.is_null() {
            backoff.spin();
            next = unsafe { (*head).next.load(Ordering::Relaxed) };
        }

        self.head.set(next);
        // Deallocate previous node.
        let _ = unsafe { Box::from_raw(head) };

        Ok(unsafe { (*next).queue.recv((*next).queue.cast()).unwrap() })
    }

    pub(crate) fn close(&self) {
        let next = Box::into_raw(Box::new(SpscNode::new(1)));
        unsafe { (*next).queue.close(); }

        set_close!(self.tail, self.closed, next);

        let mut head = self.head.get();
        close_current_node!(head);
    }
}

//=================
//      MPMC
//=================

struct MpmcNode<T> {
    queue: MpmcBounded<T>,
    next: AtomicPtr<MpmcNode<T>>,
}

impl<T> MpmcNode<T> {
    #[inline]
    fn new(cap: u32) -> Self {
        Self {
            queue: MpmcBounded::new(cap),
            next: AtomicPtr::new(null_mut()),
        }
    }
}

pub(crate) struct MpmcUnbounded<T> {
    head: CachePadded<AtomicPtr<MpmcNode<T>>>,
    tail: CachePadded<AtomicPtr<MpmcNode<T>>>,

    closed: CachePadded<AtomicBool>,
}

impl<T> Default for MpmcUnbounded<T> {
    #[inline]
    fn default() -> Self {
        let init_node = Box::into_raw(Box::new(MpmcNode::new(SUB_QUEUE_SIZE)));
        Self {
            head: CachePadded::new(AtomicPtr::new(init_node)),
            tail: CachePadded::new(AtomicPtr::new(init_node)),
            closed: CachePadded::new(AtomicBool::new(false)),
        }
    }
}

impl<T> MpmcUnbounded<T> {
    pub(crate) fn send(&mut self, msg: T) -> Result<(), SendError<T>> {
        let mut data = msg;
        let backoff = Backoff::default();

        loop {
            let tail = self.tail.load(Ordering::Relaxed);
            let result = unsafe { (*tail).queue.try_send(data) };

            match result {
                Ok(()) => {
                    return Ok(());
                }
                Err(TrySendError::Disconnected(msg)) => {
                    if self.closed.load(Ordering::Relaxed) {
                        return Err(SendError(msg));
                    }
                    backoff.spin();
                    data = msg;
                }
                Err(_) => {
                    let temp = Box::into_raw(Box::new(MpmcNode::new(1)));
                    match unsafe {
                        (*tail).next.compare_exchange_weak(
                            null_mut(),
                            temp,
                            Ordering::Acquire,
                            Ordering::Relaxed,
                        )
                    } {
                        Ok(_) => {
                            unsafe { (*tail).queue.close() };

                            let next = Box::into_raw(Box::new(MpmcNode::new(SUB_QUEUE_SIZE)));
                            unsafe {
                                let _ = Box::from_raw(temp);
                                let _ = (*next).queue.try_send(result.err().unwrap().into_inner());
                                (*tail).next.store(next, Ordering::Relaxed);
                            }
                            self.tail.store(next, Ordering::Release);

                            return Ok(());
                        }
                        Err(_) => unsafe {
                            let _ = Box::from_raw(temp);
                            backoff.spin();
                            data = result.err().unwrap().into_inner();
                        },
                    }
                }
            };
        }
    }

    pub(crate) fn recv(&mut self) -> Result<T, RecvError> {
        loop {
            let head = self.head.load(Ordering::Relaxed);
            let result = unsafe { (*head).queue.recv((*head).queue.cast()) };
            if likely(result.is_ok()) {
                return Ok(result.unwrap());
            }

            if unlikely(self.closed.load(Ordering::Relaxed)) {
                return Err(RecvError);
            }

            let backoff = Backoff::default();
            let mut next = unsafe { (*head).next.load(Ordering::Relaxed) };
            while next.is_null() || unsafe { (*next).queue.get_cap() } != SUB_QUEUE_SIZE {
                backoff.spin();
                next = unsafe { (*head).next.load(Ordering::Relaxed) };
            }

            if self.head.compare_exchange_weak(
                head,
                next,
                Ordering::Acquire,
                Ordering::Relaxed,
            ).is_ok() {
                unsafe { let _ = Box::from_raw(head); }
            }
        }
    }

    pub(crate) fn close(&self) {
        let next = Box::into_raw(Box::new(MpmcNode::new(1)));
        unsafe { (*next).queue.close(); }

        set_close!(self.tail, self.closed, next);
        
        let mut head = self.head.load(Ordering::Relaxed);
        close_current_node!(head);
    }
}