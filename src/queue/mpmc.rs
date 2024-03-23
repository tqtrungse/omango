use std::{
    sync::Arc,
    cell::UnsafeCell,
};

use crate::queue::{
    waker::Checker,
    unbounded::MpmcUnbounded,
    bounded::{Bounded, MpmcBounded},
    error::{SendError, RecvError, TryRecvError, TrySendError},
};

/// Creates a queue of bounded capacity.
///
/// This queue has a buffer that can hold at most cap messages at a time.
///
/// A special case is zero-capacity queue, which cannot hold any messages.
/// Instead, send and receive operations must appear at the same time
/// in order to pair up and pass the message over:
///
/// # Examples
///
/// A queue of capacity 1:
///
/// ```
/// use std::thread;
/// use std::time::Duration;
///
/// use omango::queue::mpmc::bounded;
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
/// use omango::queue::mpmc::bounded;
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
#[inline]
pub fn bounded<T: Send>(size: u32) -> (BSender<T>, BReceiver<T>) {
    let queue = Arc::new(UnsafeCell::new(MpmcBounded::new(size)));
    (BSender::new(queue.clone()), BReceiver::new(queue))
}

/// The sending side of a bounded queue.
///
/// # Examples
///
/// ```
/// use std::thread;
///
/// use omango::queue::mpmc::bounded;
///
/// let (tx, rx) = bounded(1);
/// let tx2 = tx.clone();
///
/// thread::spawn(move || tx.send(1).unwrap());
/// thread::spawn(move || tx2.send(2).unwrap());
///
/// let msg1 = rx.recv().unwrap();
/// let msg2 = rx.recv().unwrap();
///
/// assert_eq!(msg1 + msg2, 3);
/// ```
pub struct BSender<T> {
    inner: Arc<UnsafeCell<MpmcBounded<T>>>,
}

unsafe impl<T: Send> Send for BSender<T> {}

unsafe impl<T: Send> Sync for BSender<T> {}

impl<T: Send> Clone for BSender<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<T: Send> BSender<T> {
    #[inline]
    fn new(inner: Arc<UnsafeCell<MpmcBounded<T>>>) -> Self <> {
        Self { inner }
    }

    /// Attempts to send a message into the queue without blocking.
    ///
    /// This method will either send a message into the queue immediately or return an error if
    /// the queue is full or disconnected. The returned error contains the original message.
    ///
    /// # Examples
    ///
    /// ```
    /// use omango::queue::mpmc::bounded;
    /// use omango::queue::error::TrySendError;
    ///
    /// let (tx, rx) = bounded(0);
    ///
    /// assert_eq!(tx.try_send(1), Ok(()));
    /// assert_eq!(tx.try_send(2), Err(TrySendError::Full(2)));
    ///
    /// rx.close();
    /// assert_eq!(tx.try_send(3), Err(TrySendError::Disconnected(3)));
    /// ```
    #[inline]
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        unsafe { (*self.inner.get()).try_send(value) }
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
    /// use omango::queue::mpmc::bounded;
    /// use omango::queue::error::SendError;
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
    #[inline]
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        unsafe { (*self.inner.get()).send(value, (*self.inner.get()).cast()) }
    }

    /// Get current length of queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::mpmc::bounded;
    /// use omango::queue::error::{RecvError, SendError};
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
    ///
    /// [`send`]: BSender::send
    /// [`try_send`]: BSender::try_send
    /// [`recv`]: BReceiver::recv
    /// [`try_recv`]: BReceiver::try_recv
    #[inline]
    pub fn length(&self) -> u32 {
        unsafe { (*self.inner.get()).length() }
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
    /// use omango::queue::mpmc::bounded;
    /// use omango::queue::error::{RecvError, SendError};
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
    /// [`send`]: BSender::send
    /// [`try_send`]: BSender::try_send
    /// [`recv`]: BReceiver::recv
    /// [`try_recv`]: BReceiver::try_recv
    #[inline]
    pub fn close(&self) {
        unsafe { (*self.inner.get()).close() }
    }

    /// Check the queue was closed.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::mpmc::bounded;
    /// use omango::queue::error::{RecvError, SendError};
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
    #[inline]
    pub fn is_close(&self) -> bool {
        unsafe { (*self.inner.get()).is_close() }
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
/// use omango::queue::mpmc::bounded;
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
    inner: Arc<UnsafeCell<MpmcBounded<T>>>,
}

unsafe impl<T: Send> Send for BReceiver<T> {}

unsafe impl<T: Send> Sync for BReceiver<T> {}

impl<T: Send> Clone for BReceiver<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<T: Send> BReceiver<T> {
    #[inline]
    fn new(inner: Arc<UnsafeCell<MpmcBounded<T>>>) -> Self <> {
        Self { inner }
    }

    /// Attempts to receive a message from the queue without blocking.
    ///
    /// This method will either receive a message from the queue immediately or return an error
    /// if the queue is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use omango::queue::mpmc::bounded;
    /// use omango::queue::error::TryRecvError;
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
    #[inline]
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        unsafe { (*self.inner.get()).try_recv() }
    }

    /// Blocks the current thread until a message is received or the queue is empty and
    /// disconnected.
    ///
    /// If the queue is empty and not disconnected, this call will block until the reception
    /// operation can proceed. If the queue is empty and becomes disconnected, this call will
    /// wake up and return an error.
    ///
    /// If called on a zero-capacity queue, this method will wait for a send operation to appear
    /// on the other side of the queue.
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::mpmc::bounded;
    /// use omango::queue::error::RecvError;
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
    #[inline]
    pub fn recv(&self) -> Result<T, RecvError> {
        unsafe { (*self.inner.get()).recv((*self.inner.get()).cast()) }
    }

    /// Get current length of queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::mpmc::bounded;
    /// use omango::queue::error::{RecvError, SendError};
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
    ///
    /// [`send`]: BSender::send
    /// [`try_send`]: BSender::try_send
    /// [`recv`]: BReceiver::recv
    /// [`try_recv`]: BReceiver::try_recv
    #[inline]
    pub fn length(&self) -> u32 {
        unsafe { (*self.inner.get()).length() }
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
    /// use omango::queue::mpmc::bounded;
    /// use omango::queue::error::{RecvError, SendError};
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
    /// [`send`]: BSender::send
    /// [`try_send`]: BSender::try_send
    /// [`recv`]: BReceiver::recv
    /// [`try_recv`]: BReceiver::try_recv
    #[inline]
    pub fn close(&self) {
        unsafe { (*self.inner.get()).close() }
    }

    /// Check the queue was closed.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::mpmc::bounded;
    /// use omango::queue::error::{RecvError, SendError};
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
    #[inline]
    pub fn is_close(&self) -> bool {
        unsafe { (*self.inner.get()).is_close() }
    }
}

//==============
//  UNBOUNDED
//==============

/// Creates a queue of unbounded capacity.
///
/// This queue can push unlimited message.
///
///
/// # Examples
///
/// ```
/// use std::thread;
/// use std::time::Duration;
///
/// use omango::queue::mpmc::unbounded;
///
/// let (tx, rx) = unbounded();
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
#[inline]
pub fn unbounded<T: Send>() -> (USender<T>, UReceiver<T>) {
    let queue = Arc::new(UnsafeCell::new(MpmcUnbounded::default()));
    (USender::new(queue.clone()), UReceiver::new(queue))
}

/// The sending side of unbounded queue.
///
/// # Examples
///
/// ```
/// use std::thread;
///
/// use omango::queue::mpmc::unbounded;
///
/// let (tx, rx) = unbounded();
/// let tx2 = tx.clone();
///
/// thread::spawn(move || tx.send(1).unwrap());
/// thread::spawn(move || tx2.send(2).unwrap());
///
/// let msg1 = rx.recv().unwrap();
/// let msg2 = rx.recv().unwrap();
///
/// assert_eq!(msg1 + msg2, 3);
/// ```
pub struct USender<T> {
    inner: Arc<UnsafeCell<MpmcUnbounded<T>>>,
}

unsafe impl<T: Send> Send for USender<T> {}

unsafe impl<T: Send> Sync for USender<T> {}

impl<T: Send> Clone for USender<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<T: Send> USender<T> {
    #[inline]
    fn new(inner: Arc<UnsafeCell<MpmcUnbounded<T>>>) -> Self <> {
        Self { inner }
    }

    /// Attempts to send a message into the queue without blocking.
    ///
    /// This method will either send a message into the queue immediately or return an error if
    /// the queue is full or disconnected. The returned error contains the original message.
    ///
    /// # Examples
    ///
    /// ```
    /// use omango::queue::mpmc::unbounded;
    /// use omango::queue::error::SendError;
    ///
    /// let (tx, rx) = unbounded();
    ///
    /// assert_eq!(tx.send(1), Ok(()));
    ///
    /// rx.close();
    /// assert_eq!(tx.send(3), Err(SendError(3)));
    /// ```
    #[inline]
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        unsafe { (*self.inner.get()).send(value) }
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
    /// use omango::queue::mpmc::unbounded;
    /// use omango::queue::error::{RecvError, SendError};
    ///
    /// let (tx, rx) = unbounded();
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
    /// [`send`]: USender::send
    /// [`recv`]: UReceiver::recv
    #[inline]
    pub fn close(&self) {
        unsafe { (*self.inner.get()).close() }
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
/// use omango::queue::mpmc::unbounded;
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
    inner: Arc<UnsafeCell<MpmcUnbounded<T>>>,
}

unsafe impl<T: Send> Send for UReceiver<T> {}

unsafe impl<T: Send> Sync for UReceiver<T> {}

impl<T: Send> Clone for UReceiver<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<T: Send> UReceiver<T> {
    #[inline]
    fn new(inner: Arc<UnsafeCell<MpmcUnbounded<T>>>) -> Self <> {
        Self { inner }
    }

    /// Blocks the current thread until a message is received or the queue is empty and
    /// disconnected.
    ///
    /// If the queue is empty and not disconnected, this call will block until the reception
    /// operation can proceed. If the queue is empty and becomes disconnected, this call will
    /// wake up and return an error.
    ///
    /// If called on a zero-capacity queue, this method will wait for a send operation to appear
    /// on the other side of the queue.
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// use omango::queue::mpmc::unbounded;
    /// use omango::queue::error::RecvError;
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
    #[inline]
    pub fn recv(&self) -> Result<T, RecvError> {
        unsafe { (*self.inner.get()).recv() }
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
    /// use omango::queue::mpmc::unbounded;
    /// use omango::queue::error::{RecvError, SendError};
    ///
    /// let (tx, rx) = unbounded();
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
    /// [`send`]: USender::send
    /// [`recv`]: UReceiver::recv
    #[inline]
    pub fn close(&self) {
        unsafe { (*self.inner.get()).close() }
    }
}

mod test {
    use crate::queue::mpmc::{bounded, BReceiver, BSender, unbounded, UReceiver, USender};

    fn is_send<T: Send>() {}

    #[test]
    fn bounds() {
        is_send::<BSender<i32>>();
        is_send::<BReceiver<i32>>();
    }

    #[test]
    fn unbound() {
        is_send::<USender<i32>>();
        is_send::<UReceiver<i32>>();
    }

    #[test]
    fn send_recv() {
        // Bounded.
        let (tx_b, rx_b) = bounded(3);
        tx_b.try_send(1).unwrap();
        assert_eq!(rx_b.try_recv().unwrap(), 1);

        // Unbounded
        let (tx_u, rx_u) = unbounded();
        tx_u.send(1).unwrap();
        assert_eq!(rx_u.recv().unwrap(), 1);
    }

    #[test]
    fn send_shared_recv() {
        // Bounded.
        let (tx_b1, rx_b) = bounded(4);
        let tx_b2 = tx_b1.clone();

        tx_b1.send(1).unwrap();
        assert_eq!(rx_b.recv().unwrap(), 1);

        tx_b2.send(2).unwrap();
        assert_eq!(rx_b.recv().unwrap(), 2);

        // Unbounded.
        let (tx_u1, rx_u) = unbounded();
        let tx_u2 = tx_u1.clone();

        tx_u1.send(1).unwrap();
        assert_eq!(rx_u.recv().unwrap(), 1);

        tx_u2.send(2).unwrap();
        assert_eq!(rx_u.recv().unwrap(), 2);
    }

    #[test]
    fn send_recv_threads() {
        // Bounded.
        let (tx_b, rx_b) = bounded(4);
        let thread = std::thread::spawn(move || {
            tx_b.send(1).unwrap();
        });
        assert_eq!(rx_b.recv().unwrap(), 1);
        thread.join().unwrap();

        // Unbounded.
        let (tx_u, rx_u) = unbounded();
        let thread = std::thread::spawn(move || {
            tx_u.send(1).unwrap();
        });
        assert_eq!(rx_u.recv().unwrap(), 1);
        thread.join().unwrap();
    }

    #[test]
    fn send_recv_threads_no_capacity() {
        let (tx, rx) = bounded(0);
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
        let (tx_b, rx_b) = bounded::<i32>(1);
        let thread = std::thread::spawn(move || {
            assert!(rx_b.recv().is_err());
        });
        tx_b.close();
        thread.join().unwrap();

        // Unbounded.
        let (tx_u, rx_u) = unbounded::<i32>();
        let thread = std::thread::spawn(move || {
            assert!(rx_u.recv().is_err());
        });
        tx_u.close();
        thread.join().unwrap();
    }

    #[test]
    fn mpsc_no_capacity() {
        let amt = 30000;
        let nthreads = 7;
        let (tx, rx) = bounded(0);

        for _ in 0..nthreads {
            let txc = tx.clone();
            std::thread::spawn(move || {
                for _ in 0..amt {
                    assert_eq!(txc.send(1), Ok(()));
                }
            });
        }
        for _ in 0..amt * nthreads {
            assert_eq!(rx.recv(), Ok(1));
        }
    }

    #[test]
    fn mpmc_no_capacity() {
        let amt = 50000;
        let nthreads_send = 7;
        let nthreads_recv = 7;
        let (tx, rx) = bounded(0);
        let mut receiving_threads = Vec::new();
        let mut sending_threads = Vec::new();

        for _ in 0..nthreads_send {
            let txc = tx.clone();
            let child = std::thread::spawn(move || {
                for _ in 0..amt {
                    assert_eq!(txc.send(1), Ok(()));
                }
            });
            sending_threads.push(child);
        }

        for _ in 0..nthreads_recv {
            let rxc = rx.clone();
            let thread = std::thread::spawn(move || {
                for _ in 0..amt {
                    assert_eq!(rxc.recv(), Ok(1));
                }
            });
            receiving_threads.push(thread);
        }

        for thread in sending_threads {
            thread.join().expect("oops! the child thread panicked");
        }
        for thread in receiving_threads {
            thread.join().expect("oops! the child thread panicked");
        }
    }
}