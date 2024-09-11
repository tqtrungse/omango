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
    rc::Rc,
    any::Any,
    cell::UnsafeCell,
    mem::MaybeUninit,
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use omango_util::lock::RwSpinlock;

use crate::{
    wg::WaitGroup,
    error::WrapError,
};

pub type Fn<T> = fn() -> Result<T, WrapError>;

macro_rules! get_result {
    ($call:expr) => {
        unsafe { (*$call.result.get()).assume_init_ref().clone() }
    };
}
macro_rules! set_result {
    ($call:expr, $result:expr) => {
        unsafe { $call.result.get().write(MaybeUninit::new(Arc::new($result))) }
    };
}

struct Call<T: Any> {
    wg: WaitGroup,
    count: AtomicU32,

    // These fields are written once before the WaitGroup is done
    // and are only read after the WaitGroup is done.
    result: UnsafeCell<MaybeUninit<Arc<Result<T, WrapError>>>>,
}

impl<T: Any> Default for Call<T> {
    #[inline(always)]
    fn default() -> Self {
        Self {
            wg: WaitGroup::default(),
            count: AtomicU32::new(1),
            result: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
}

/// [Group] represents a struct of work and forms a namespace in
/// which units of work can be executed with duplicate suppression.
pub struct Group {
    guard: RwSpinlock<HashMap<String, Rc<dyn Any>>>,
}

impl Default for Group {
    #[inline(always)]
    fn default() -> Self {
        Self {
            guard: RwSpinlock::new(HashMap::default()),
        }
    }
}

impl Group {
    /// Executes and returns the results of the given function, making
    /// sure that only one execution is in-flight for a given key at a
    /// time. 
    /// If a duplicate comes in, the duplicate caller waits for the original
    /// to complete and receives the same results.
    /// The return value includes `u32` indicates number of callers 
    /// (aka number times of shared value).
    ///
    /// Example:
    /// 
    /// ```
    /// use omango::single::flight::Group;
    /// 
    /// let g = std::sync::Arc::new(Group::default());
    /// let g_clone = g.clone();
    /// 
    /// let thread = std::thread::spawn(move || {
    ///     let (rs, _) = g.exec("google", move || {
    ///         std::thread::sleep(std::time::Duration::from_secs(1));
    ///         Ok(1i32)
    ///     });
    ///     match rs.as_ref() {
    ///         Ok(v) => assert_eq!(v, &1i32),
    ///         Err(_) => panic!("should be success"),
    ///    }
    /// });
    /// 
    /// let (rs, times) = g_clone.exec("google", move || {
    ///     std::thread::sleep(std::time::Duration::from_secs(1));
    ///     Ok(1i32)
    /// });
    /// thread.join().unwrap();
    /// 
    /// match rs.as_ref() {
    ///     Ok(v) => assert_eq!(v, &1i32),
    ///     Err(_) => panic!("should be success"),
    /// }
    /// assert_eq!(times, 2);
    /// 
    /// g_clone.forgot("google");
    /// ```
    pub fn exec<T: Any>(&self, key: &str, func: Fn<T>) -> (Arc<Result<T, WrapError>>, u32) {
        match self.get(key) {
            Some(any) => {
                let call = any.downcast_ref::<Call<T>>().unwrap();
                call.count.fetch_add(1, Ordering::Relaxed);
                call.wg.wait();

                (get_result!(call), call.count.load(Ordering::Relaxed))
            }
            None => {
                let oc = Rc::<Call<T>>::default();
                let call = oc.clone();
                oc.wg.add(1);
                self.guard.write().insert(key.to_string(), oc);

                let result = panic::catch_unwind(|| {
                    func()
                });
                let out = match result {
                    Ok(result) => {
                        set_result!(call, result);
                        (get_result!(call), call.count.load(Ordering::Relaxed))
                    }
                    Err(_) => {
                        set_result!(call, Err(WrapError("function of user panic".to_string())));
                        (get_result!(call), call.count.load(Ordering::Relaxed))
                    }
                };
                call.wg.done();
                out
            }
        }
    }

    /// Tells the single-flight to forget about a key. Future calls
    /// to [`exec`] for this key will call the function rather than waiting for
    /// an earlier call to complete.
    /// 
    /// NOTE: If it cannot call, the future calls will get result of the
    /// last calling.
    /// 
    /// [`exec`]: Group::exec
    #[inline(always)]
    pub fn forgot(&self, key: &str) -> bool {
        self.guard.write().remove(key).is_some()
    }

    #[allow(clippy::map_clone)]
    #[inline(always)]
    fn get(&self, key: &str) -> Option<Rc<dyn Any>> {
        self.guard.read().get(key).map(|v| v.clone())
    }
}

unsafe impl Send for Group {}
unsafe impl Sync for Group {}

mod test {
    #[test]
    fn test() {
        let g = std::sync::Arc::new(crate::single::flight::Group::default());
        let g_clone = g.clone();

        let thread = std::thread::spawn(move || {
            let (rs, _) = g.exec("google", move || {
                std::thread::sleep(std::time::Duration::from_secs(1));
                Ok(1i32)
            });
            match rs.as_ref() {
                Ok(v) => assert_eq!(v, &1i32),
                Err(_) => panic!("should be success"),
            }
        });

        let (rs, times) = g_clone.exec("google", move || {
            std::thread::sleep(std::time::Duration::from_secs(1));
            Ok(1i32)
        });
        thread.join().unwrap();

        match rs.as_ref() {
            Ok(v) => assert_eq!(v, &1i32),
            Err(_) => panic!("should be success"),
        }
        assert_eq!(times, 2);

        g_clone.forgot("google");
    }
}