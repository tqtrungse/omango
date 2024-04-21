use std::{
    sync::Arc,
    cell::UnsafeCell,
};

use crate::{
    queue::{
        waker::Checker,
        unbounded::SpscUnbounded,
        bounded::{Bounded, SpscBounded},
    },
    error::{SendError, RecvError, TrySendError, TryRecvError},
};

/// Creates a queue of bounded capacity.
///
/// This queue has a buffer that can hold at most cap messages at a time.
///
/// A special case is zero-capacity queue, which cannot hold any messages.
/// Instead, send and receive operations must appear at the same time
/// in order to pair up and pass the message over.
///
/// # Examples
///
/// A queue of capacity 1:
///
/// ```
/// use std::thread;
/// use std::time::Duration;
///
/// use omango::queue::spsc::bounded;
///
/// let (tx, rx) = bounded(1);
///
/// // This call returns immediately because there is enough space in the queue.
/// tx.send(1).unwrap();
///
/// thread::spawn(move || {
///     // This call blocks the current thread because the queue is full.
///     // It will be able to complete only after the first message is received.
///     tx.send(2).unwrap();
/// });
///
/// thread::sleep(Duration::from_secs(1));
/// assert_eq!(rx.recv(), Ok(1));
/// assert_eq!(rx.recv(), Ok(2));
/// ```
///
/// A zero-capacity queue:
///
/// ```
/// use std::thread;
/// use std::time::Duration;
///
/// use omango::queue::spsc::bounded;
///
/// let (tx, rx) = bounded(0);
///
/// thread::spawn(move || {
///     // This call blocks the current thread until a reception operation appears
///     // on the other side of the queue.
///     tx.send(1).unwrap();
/// });
///
/// thread::sleep(Duration::from_secs(1));
/// assert_eq!(rx.recv(), Ok(1));
/// ```
#[inline(always)]
pub fn bounded<T: Send>(size: u32) -> (BSender<T>, BReceiver<T>) {
    let queue = Arc::new(UnsafeCell::new(SpscBounded::new(size)));
    (BSender::new(queue.clone()), BReceiver::new(queue))
}

/// The sending side of a queue.
///
/// # Examples
///
/// ```
/// use std::thread;
///
/// use omango::queue::spsc::bounded;
///
/// let (tx, rx) = bounded(1);
///
/// thread::spawn(move || tx.send(1).unwrap());
///
/// let msg1 = rx.recv().unwrap();
///
/// assert_eq!(msg1, 1);
/// ```
pub struct BSender<T> {
    core: Arc<UnsafeCell<SpscBounded<T>>>,
}

unsafe impl<T: Send> Send for BSender<T> {}

unsafe impl<T: Send> Sync for BSender<T> {}

impl<T: Send> Clone for BSender<T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        Self { core: self.core.clone() }
    }
}

impl<T: Send> BSender<T> {
    #[inline(always)]
    fn new(inner: Arc<UnsafeCell<SpscBounded<T>>>) -> Self <> {
        Self { core: inner }
    }

    /// Attempts to send a message into the queue without blocking.
    ///
    /// This method will either send a message into the queue immediately or return an error if
    /// the queue is full or disconnected. The returned error contains the original message.
    ///
    /// # Examples
    ///
    /// ```
    /// use omango::queue::spsc::bounded;
    /// use omango::error::TrySendError;
    ///
    /// let (tx, rx) = bounded(0);
    ///
    /// assert_eq!(tx.try_send(1), Ok(()));
    /// assert_eq!(tx.try_send(2), Err(TrySendError::Full(2)));
    ///
    /// rx.close();
    /// assert_eq!(tx.try_send(3), Err(TrySendError::Disconnected(3)));
    /// ```
    #[inline(always)]
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        unsafe { (*self.core.get()).try_send(value) }
    }

    /// Blocks the current thread until a message is sent or the queue is disconnected.
    ///
    /// If the queue is full and not disconnected, this call will block until the send operation
    /// can proceed. If the queue becomes disconnected, this call will wake up and return an
    /// error. The returned error contains the original message.
    ///
    /// If called on a zero-capacity queue, this method will wait for a reception operation to
    /// appear on the other side of the queue.
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::spsc::bounded;
    /// use omango::error::SendError;
    ///
    /// let (tx, rx) = bounded(0);
    /// assert_eq!(tx.send(1), Ok(()));
    ///
    /// thread::spawn(move || {
    ///     assert_eq!(rx.recv(), Ok(1));
    ///     thread::sleep(Duration::from_secs(1));
    ///     rx.close();
    /// });
    ///
    /// assert_eq!(tx.send(2), Ok(()));
    /// assert_eq!(tx.send(3), Err(SendError(3)));
    /// ```
    #[inline(always)]
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        unsafe { (*self.core.get()).send(value, (*self.core.get()).cast()) }
    }

    /// Get current length of queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::spsc::bounded;
    /// use omango::error::{RecvError, SendError};
    ///
    /// let (tx, rx) = bounded(0);
    /// let tx2 = tx.clone();
    ///
    /// thread::spawn(move || {
    ///     assert_eq!(tx.send(1), Ok(()));
    ///     tx.close();
    /// });
    ///
    /// thread::sleep(Duration::from_millis(500));
    /// assert_eq!(tx2.length(), 1);
    /// ```
    #[inline(always)]
    pub fn length(&self) -> u32 {
        unsafe { (*self.core.get()).length() }
    }

    /// Fires closing queue notification.
    ///
    /// After closed, all [`try_send`] and [`send`] operations will be failed.
    ///
    /// Uses [`try_recv`] or [`recv`] to read remaining messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::spsc::bounded;
    /// use omango::error::{RecvError, SendError};
    ///
    /// let (tx, rx) = bounded(0);
    ///
    /// thread::spawn(move || {
    ///     assert_eq!(tx.send(1), Ok(()));
    ///     tx.close();
    /// });
    ///
    /// thread::sleep(Duration::from_millis(500));
    /// assert_eq!(rx.recv(), Ok(1)); // Or use try_recv.
    /// ```
    ///
    /// [`send`]: spsc::BSender::send
    /// [`try_send`]: spsc::BSender::try_send
    /// [`recv`]: spsc::BReceiver::recv
    /// [`try_recv`]: spsc::BReceiver::try_recv
    #[inline(always)]
    pub fn close(&self) {
        unsafe { (*self.core.get()).close() }
    }

    /// Check the queue was closed.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::spsc::bounded;
    /// use omango::error::{RecvError, SendError};
    ///
    /// let (tx, rx) = bounded::<i32>(0);
    ///
    /// thread::spawn(move || {
    ///     tx.close();
    /// });
    ///
    /// thread::sleep(Duration::from_millis(500));
    /// assert_eq!(rx.is_close(), true);
    /// ```
    #[inline(always)]
    pub fn is_close(&self) -> bool {
        unsafe { (*self.core.get()).is_close() }
    }
}

/// The receiving side of a queue.
///
/// # Examples
///
/// ```
/// use std::thread;
/// use std::time::Duration;
///
/// use omango::queue::spsc::bounded;
///
/// let (tx, rx) = bounded(0);
///
/// thread::spawn(move || {
///     let _ = tx.send(1);
///     thread::sleep(Duration::from_secs(1));
///     let _ = tx.send(2);
/// });
///
/// assert_eq!(rx.recv(), Ok(1)); // Received immediately.
/// assert_eq!(rx.recv(), Ok(2)); // Received after 1 second.
/// ```
pub struct BReceiver<T> {
    core: Arc<UnsafeCell<SpscBounded<T>>>,
}

unsafe impl<T: Send> Send for BReceiver<T> {}

unsafe impl<T: Send> Sync for BReceiver<T> {}

impl<T: Send> Clone for BReceiver<T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        Self { core: self.core.clone() }
    }
}

impl<T: Send> BReceiver<T> {
    #[inline(always)]
    fn new(inner: Arc<UnsafeCell<SpscBounded<T>>>) -> Self <> {
        Self { core: inner }
    }

    /// Attempts to receive a message from the queue without blocking.
    ///
    /// This method will either receive a message from the queue immediately or return an error
    /// if the queue is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use omango::queue::spsc::bounded;
    /// use omango::error::TryRecvError;
    ///
    /// let (tx, rx) = bounded(1);
    /// assert_eq!(rx.try_recv(), Err(TryRecvError));
    ///
    /// tx.send(5).unwrap();
    /// tx.close();
    ///
    /// assert_eq!(rx.try_recv(), Ok(5));
    /// assert_eq!(rx.try_recv(), Err(TryRecvError));
    /// ```
    #[inline(always)]
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        unsafe { (*self.core.get()).try_recv() }
    }

    /// Blocks the current thread until a message is received.
    ///
    /// If the queue is empty and not disconnected, this call will block until the reception
    /// operation can proceed. If the queue becomes disconnected, this call will be woken up
    /// then it will retry if the queue is still empty, an error will return.
    ///
    /// If called on a zero-capacity queue, this method will wait for a send operation to appear
    /// on the other side of the queue.
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::spsc::bounded;
    /// use omango::error::RecvError;
    ///
    /// let (tx, rx) = bounded(1);
    ///
    /// thread::spawn(move || {
    ///     thread::sleep(Duration::from_secs(1));
    ///     tx.send(5).unwrap();
    ///     thread::sleep(Duration::from_secs(1));
    ///     tx.close();
    /// });
    ///
    /// assert_eq!(rx.recv(), Ok(5));
    /// assert_eq!(rx.recv(), Err(RecvError));
    /// ```
    #[inline(always)]
    pub fn recv(&self) -> Result<T, RecvError> {
        unsafe { (*self.core.get()).recv((*self.core.get()).cast()) }
    }

    /// Get current length of queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::spsc::bounded;
    /// use omango::error::{RecvError, SendError};
    ///
    /// let (tx, rx) = bounded(0);
    /// let tx2 = tx.clone();
    ///
    /// thread::spawn(move || {
    ///     thread::sleep(Duration::from_secs(1));
    ///     assert_eq!(tx.send(1), Ok(()));
    ///     tx.close();
    /// });
    ///
    /// assert_eq!(tx2.length(), 1);
    /// ```
    #[inline(always)]
    pub fn length(&self) -> u32 {
        unsafe { (*self.core.get()).length() }
    }

    /// Fires closing queue notification.
    ///
    /// After closed, all [`try_send`] and [`send`] operations will be failed.
    ///
    /// Uses [`try_recv`] or [`recv`] to read remaining messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::spsc::bounded;
    /// use omango::error::{RecvError, SendError};
    ///
    /// let (tx, rx) = bounded(0);
    ///
    /// thread::spawn(move || {
    ///     assert_eq!(tx.send(1), Ok(()));
    ///     tx.close();
    /// });
    ///
    /// thread::sleep(Duration::from_millis(500));
    /// assert_eq!(rx.recv(), Ok(1)); // Or use try_recv.
    /// ```
    ///
    /// [`send`]: spsc::BSender::send
    /// [`try_send`]: spsc::BSender::try_send
    /// [`recv`]: spsc::BReceiver::recv
    /// [`try_recv`]: spsc::BReceiver::try_recv
    #[inline(always)]
    pub fn close(&self) {
        unsafe { (*self.core.get()).close() }
    }

    /// Check the queue was closed.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::spsc::bounded;
    /// use omango::error::{RecvError, SendError};
    ///
    /// let (tx, rx) = bounded::<i32>(0);
    ///
    /// thread::spawn(move || {
    ///     tx.close();
    /// });
    ///
    /// thread::sleep(Duration::from_millis(500));
    /// assert_eq!(rx.is_close(), true);
    /// ```
    #[inline(always)]
    pub fn is_close(&self) -> bool {
        unsafe { (*self.core.get()).is_close() }
    }
}

//======================
//      UNBOUNDED
//======================

/// Creates a queue of unbounded capacity.
/// 
/// # Examples
///
///
/// ```
/// use std::thread;
/// use std::time::Duration;
///
/// use omango::queue::spsc::unbounded;
///
/// let (tx, rx) = unbounded();
///
/// // This call returns immediately because there is enough space in the queue.
/// tx.send(1).unwrap();
///
/// thread::spawn(move || {
///     tx.send(2).unwrap();
/// });
///
/// thread::sleep(Duration::from_secs(1));
/// assert_eq!(rx.recv(), Ok(1));
/// assert_eq!(rx.recv(), Ok(2));
/// ```
#[inline(always)]
pub fn unbounded<T: Send>() -> (USender<T>, UReceiver<T>) {
    let queue = Arc::new(UnsafeCell::new(SpscUnbounded::default()));
    (USender::new(queue.clone()), UReceiver::new(queue))
}

/// The sending side of unbounded queue.
///
/// # Examples
///
/// ```
/// use std::thread;
///
/// use omango::queue::spsc::unbounded;
///
/// let (tx, rx) = unbounded();
///
/// thread::spawn(move || tx.send(1).unwrap());
///
/// let msg1 = rx.recv().unwrap();
///
/// assert_eq!(msg1, 1);
/// ```
pub struct USender<T> {
    core: Arc<UnsafeCell<SpscUnbounded<T>>>,
}

unsafe impl<T: Send> Send for USender<T> {}

unsafe impl<T: Send> Sync for USender<T> {}

impl<T: Send> Clone for USender<T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        Self { core: self.core.clone() }
    }
}

impl<T: Send> USender<T> {
    #[inline(always)]
    fn new(inner: Arc<UnsafeCell<SpscUnbounded<T>>>) -> Self <> {
        Self { core: inner }
    }

    /// Attempts to send a message into the queue without blocking.
    /// 
    /// This operation will be failed when queue was closed.
    /// 
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::spsc::unbounded;
    ///
    /// let (tx, rx) = unbounded();
    /// tx.send(1).unwrap();
    ///
    /// thread::spawn(move || {
    ///     assert_eq!(rx.recv(), Ok(1));
    ///     thread::sleep(Duration::from_secs(1));
    /// });
    ///
    /// tx.send(2).unwrap();
    /// ```
    #[inline(always)]
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        unsafe { (*self.core.get()).send(value) }
    }

    /// Fires closing queue notification.
    ///
    /// After closed, all [`send`] operations will be failed.
    ///
    /// Uses [`recv`] to read remaining messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::spsc::unbounded;
    /// use omango::error::{RecvError, SendError};
    ///
    /// let (tx, rx) = unbounded();
    ///
    /// thread::spawn(move || {
    ///     assert_eq!(tx.send(1), Ok(()));
    ///     tx.close();
    /// });
    ///
    /// thread::sleep(Duration::from_millis(500));
    /// assert_eq!(rx.recv(), Ok(1));
    /// ```
    ///
    /// [`send`]: spsc::USender::send
    /// [`recv`]: spsc::UReceiver::recv
    #[inline(always)]
    pub  fn close(&self) {
        unsafe { (*self.core.get()).close() }
    }
}

/// The receiving side of unbounded queue.
///
/// # Examples
///
/// ```
/// use std::thread;
/// use std::time::Duration;
///
/// use omango::queue::spsc::unbounded;
///
/// let (tx, rx) = unbounded();
///
/// thread::spawn(move || {
///     let _ = tx.send(1);
///     thread::sleep(Duration::from_secs(1));
///     let _ = tx.send(2);
/// });
///
/// assert_eq!(rx.recv(), Ok(1)); // Received immediately.
/// assert_eq!(rx.recv(), Ok(2)); // Received after 1 second.
/// ```
pub struct UReceiver<T> {
    core: Arc<UnsafeCell<SpscUnbounded<T>>>,
}

unsafe impl<T: Send> Send for UReceiver<T> {}

unsafe impl<T: Send> Sync for UReceiver<T> {}

impl<T: Send> Clone for UReceiver<T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        Self { core: self.core.clone() }
    }
}

impl<T: Send> UReceiver<T> {
    #[inline(always)]
    fn new(inner: Arc<UnsafeCell<SpscUnbounded<T>>>) -> Self <> {
        Self { core: inner }
    }

    /// Blocks the current thread until a message is received.
    ///
    /// When queue was closed, all receiving waiters will be woken up and
    /// return error.
    /// 
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::spsc::unbounded;
    /// use omango::error::RecvError;
    ///
    /// let (tx, rx) = unbounded();
    ///
    /// thread::spawn(move || {
    ///     thread::sleep(Duration::from_secs(1));
    ///     tx.send(5).unwrap();
    ///     thread::sleep(Duration::from_secs(1));
    ///     tx.close();
    /// });
    ///
    /// assert_eq!(rx.recv(), Ok(5));
    /// assert_eq!(rx.recv(), Err(RecvError));
    /// ```
    #[inline(always)]
    pub fn recv(&self) -> Result<T, RecvError> {
        unsafe { (*self.core.get()).recv() }
    }

    /// Fires closing queue notification.
    ///
    /// After closed, all [`send`] operations will be failed.
    ///
    /// Uses [`recv`] to read remaining messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::spsc::unbounded;
    /// use omango::error::{RecvError, SendError};
    ///
    /// let (tx, rx) = unbounded();
    ///
    /// thread::spawn(move || {
    ///     assert_eq!(tx.send(1), Ok(()));
    ///     tx.close();
    /// });
    ///
    /// thread::sleep(Duration::from_millis(500));
    /// assert_eq!(rx.recv(), Ok(1));
    /// ```
    ///
    /// [`send`]: spsc::USender::send
    /// [`recv`]: spsc::UReceiver::recv
    #[inline(always)]
    pub  fn close(&self) {
        unsafe { (*self.core.get()).close() }
    }
}

mod test {
    #[test]
    fn bounds() {
        fn is_send<T: Send>() {}
        
        is_send::<crate::queue::spsc::BSender<i32>>();
        is_send::<crate::queue::spsc::BReceiver<i32>>();
    }

    #[test]
    fn unbound() {
        fn is_send<T: Send>() {}
        
        is_send::<crate::queue::spsc::USender<i32>>();
        is_send::<crate::queue::spsc::UReceiver<i32>>();
    }

    #[test]
    fn send_recv() {
        // Bounded.
        let (tx_b, rx_b) = crate::queue::spsc::bounded(3);
        tx_b.try_send(1).unwrap();
        assert_eq!(rx_b.try_recv().unwrap(), 1);

        // Unbounded
        let (tx_u, rx_u) = crate::queue::spsc::unbounded();
        tx_u.send(1).unwrap();
        assert_eq!(rx_u.recv().unwrap(), 1);
    }

    #[test]
    fn send_shared_recv() {
        // Bounded.
        let (tx_b1, rx_b) = crate::queue::spsc::bounded(4);
        let tx_b2 = tx_b1.clone();

        tx_b1.send(1).unwrap();
        assert_eq!(rx_b.recv().unwrap(), 1);

        tx_b2.send(2).unwrap();
        assert_eq!(rx_b.recv().unwrap(), 2);

        // Unbounded.
        let (tx_u1, rx_u) = crate::queue::spsc::unbounded();
        let tx_u2 = tx_u1.clone();

        tx_u1.send(1).unwrap();
        assert_eq!(rx_u.recv().unwrap(), 1);

        tx_u2.send(2).unwrap();
        assert_eq!(rx_u.recv().unwrap(), 2);
    }

    #[test]
    fn send_recv_threads() {
        // Bounded.
        let (tx_b, rx_b) = crate::queue::spsc::bounded(4);
        let thread = std::thread::spawn(move || {
            tx_b.send(1).unwrap();
        });
        assert_eq!(rx_b.recv().unwrap(), 1);
        thread.join().unwrap();

        // Unbounded.
        let (tx_u, rx_u) = crate::queue::spsc::unbounded();
        let thread = std::thread::spawn(move || {
            tx_u.send(1).unwrap();
        });
        assert_eq!(rx_u.recv().unwrap(), 1);
        thread.join().unwrap();
    }

    #[test]
    fn send_recv_threads_no_capacity() {
        let (tx, rx) = crate::queue::spsc::bounded(0);
        let thread = std::thread::spawn(move || {
            tx.send(1).unwrap();
            tx.send(2).unwrap();
        });

        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(rx.recv().unwrap(), 1);

        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(rx.recv().unwrap(), 2);

        thread.join().unwrap();
    }

    #[test]
    fn send_close_gets_none() {
        // Bounded.
        let (tx_b, rx_b) = crate::queue::spsc::bounded::<i32>(1);
        let thread = std::thread::spawn(move || {
            assert!(rx_b.recv().is_err());
        });
        tx_b.close();
        thread.join().unwrap();

        // Unbounded.
        let (tx_u, rx_u) = crate::queue::spsc::unbounded::<i32>();
        let thread = std::thread::spawn(move || {
            assert!(rx_u.recv().is_err());
        });
        std::thread::sleep(std::time::Duration::from_millis(1000));
        tx_u.close();
        thread.join().unwrap();
    }

    #[test]
    fn spsc_no_capacity() {
        let amt = 30000;
        let (tx, rx) = crate::queue::spsc::bounded(0);

        let txc = tx.clone();
        std::thread::spawn(move || {
            for _ in 0..amt {
                assert_eq!(txc.send(1), Ok(()));
            }
        });

        for _ in 0..amt {
            assert_eq!(rx.recv(), Ok(1));
        }
    }
}