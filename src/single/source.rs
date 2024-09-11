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
    panic,
    any::Any,
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{
    wg::WaitGroup,
    error::WrapError,
};

pub type Fn<T> = fn() -> Result<T, WrapError>;

macro_rules! set_result {
    ($call:expr, $data:expr) => {
        unsafe{ $call.result.get().write(MaybeUninit::new(Ok(Response(Box::new($data))))) }
    };
}

macro_rules! set_error {
    ($call:expr, $str:expr) => {
        unsafe { $call.result.get().write(MaybeUninit::new(Err(WrapError($str)))) }
    };
}

struct Call {
    key: String,
    result: UnsafeCell<MaybeUninit<Result<Response, WrapError>>>,
}

impl Default for Call {
    #[inline(always)]
    fn default() -> Self {
        Self {
            key: String::default(),
            result: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
}

pub struct Response(Box<dyn Any>);

impl<T: Any> AsRef<T> for Response {
    #[inline(always)]
    fn as_ref(&self) -> &T {
        self.0.downcast_ref::<T>().unwrap()
    }
}

impl<T: Any> AsMut<T> for Response {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut T {
        self.0.downcast_mut::<T>().unwrap()
    }
}

/// A [`Group`] waits for a collection of sources to complete.
/// After creating, the main thread or sub-thread calls [`add`] 
/// to add a collecting source. 
/// 
/// Then the main thread calls [`sum`] to wait until finished.
/// 
/// A call to [`get`] to retrieve response from a specific source.
///
/// A [`Group`] must not be copied after first use.
///
/// A call to [`reset`] clean previous data and ready for
/// the new collection.
///
/// [`add`]: Group::add
/// [`get`]: Group::get
/// [`sum`]: Group::sum
/// [`reset`]: Group::reset
pub struct Group {
    calls: UnsafeCell<Box<[Call]>>,
    index: AtomicU32,
    wg: WaitGroup,
}

impl Group {
    /// [`new`] creates a new [`Group`].
    ///
    /// The `n` params is a number of sources need to collect.
    #[inline]
    pub fn new(n: u32) -> Self {
        let calls: Box<[Call]> =
            (0..n)
                .map(|_i| Call::default())
                .collect();

        Self {
            calls: UnsafeCell::new(calls),
            index: AtomicU32::default(),
            wg: WaitGroup::new(n),
        }
    }

    /// [`add`] inserts a new source need to collect to [`Group`].
    ///
    /// The `key` param is the name of source.
    /// It has to be unique.
    /// The `func` param is a collected function is defined by user.
    ///
    ///
    /// Example see [`get`]
    ///
    /// [`get`]: Group::get
    #[inline]
    pub fn add<T: Any>(&self, key: &str, func: Fn<T>) {
        let index = self.index.fetch_add(1, Ordering::Relaxed);
        assert!(index < unsafe { (*self.calls.get()).len() } as u32);

        let result = panic::catch_unwind(|| {
            func()
        });

        let call = unsafe { (*self.calls.get()).get_unchecked_mut(index as usize) };
        call.key = key.to_string();

        match result {
            Ok(result) => match result {
                Ok(data) => set_result!(call, data),
                Err(err) => set_error!(call, err.to_string()),
            },
            Err(_) => set_error!(call, "function of user panic".to_string()),
        }
        self.wg.done();
    }

    /// [`get`] a return result from the specific source.
    /// If user call [`get`] over registered times, will be
    /// panicked.
    /// 
    /// 
    /// Example:
    /// ```
    /// use omango::single::source::Group;
    /// 
    /// let g = std::sync::Arc::new(Group::new(2));
    /// let g_clone = g.clone();
    /// 
    /// let thread = std::thread::spawn(move || {
    ///    g_clone.add("source-1", || {
    ///         Ok(1i32)
    ///     });
    /// });
    /// 
    /// g.add("source-2", || {
    ///    Ok(2i32)
    /// });
    /// 
    /// g.sum();
    /// thread.join().unwrap();
    /// 
    /// match g.get("source-1") {
    ///     Ok(resp) => {
    ///         let v: &i32 = resp.as_ref();
    ///         assert_eq!(v, &1i32);
    ///     },
    ///    Err(_) => panic!()
    /// }
    /// 
    /// match g.get("source-2") {
    ///    Ok(resp) => {
    ///         let v: &i32 = resp.as_ref();
    ///         assert_eq!(v, &2i32);
    ///    },
    ///    Err(_) => panic!()
    /// }
    /// ```
    #[inline]
    pub fn get(&self, key: &str) -> Result<Response, WrapError> {
        let calls = self.calls.get();
        let slice = unsafe { (*calls).as_ref() };
        for call in slice {
            if call.key.eq(&key) {
                return unsafe { call.result.get().read().assume_init() };
            }
        }
        Err(WrapError("key doesn't exist".to_string()))
    }

    /// [`sum`] waits until collecting from all sources completed.
    /// One [`Group`] should only call [`sum`] one time.
    /// 
    /// Example  see [`get`]
    ///
    /// [`get`]: Group::get
    #[inline(always)]
    pub fn sum(&self) {
        self.wg.wait();
    }

    /// [`reset`] clear all properties.
    /// All previous data will be destroyed. 
    /// Only calls [`reset`] after [`sum`] finished unless it 
    /// will lead to unpredictable errors.
    /// 
    /// [`sum`]: Group::sum
    #[inline]
    pub fn reset(&mut self, n: u32) {
        let calls: Box<[Call]> =
            (0..n)
                .map(|_i| Call::default())
                .collect();
        self.calls = UnsafeCell::new(calls);
        self.index = AtomicU32::new(n);
        self.wg = WaitGroup::new(n);
    }
}

unsafe impl Send for Group {}

unsafe impl Sync for Group {}

mod test {
    #[test]
    fn test() {
        let g = std::sync::Arc::new(crate::single::source::Group::new(2));
        let g_clone = g.clone();

        let thread = std::thread::spawn(move || {
            g_clone.add("source-1", || {
                Ok(1i32)
            });
        });

        g.add("source-2", || {
            Ok(2i32)
        });

        g.sum();
        thread.join().unwrap();

        match g.get("source-1") {
            Ok(resp) => {
                let v: &i32 = resp.as_ref();
                assert_eq!(v, &1i32);
            },
            Err(_) => panic!()
        }

        match g.get("source-2") {
            Ok(resp) => {
                let v: &i32 = resp.as_ref();
                assert_eq!(v, &2i32);
            },
            Err(_) => panic!()
        }
    }
    
    #[test]
    fn test_over_times() {
        let result = std::panic::catch_unwind(|| {
            let g = crate::single::source::Group::new(1);
    
            g.add("source-1", || {
                Ok(2i32)
            });
            
            g.add("source-2", || {
                Ok(2i32)
            });
            
            g.sum();
        });
        
        if result.is_ok() { panic!() }
    }
}