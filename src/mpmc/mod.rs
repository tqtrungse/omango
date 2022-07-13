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

//! Fixed size MPMC channel for message passing. It is built
//! based on a MPMC queue
//!
//! It can be an alternative to [`std::sync::mpsc`], `flume::bounded`,
//! `crossbeam_channel::bounded` (sometimes) with better performance.
//!
//! # Hello, world!
//!
//! ```
//! use omango::mpmc::bounded;
//!
//! // Create a channel of unbounded capacity.
//! let (tx, rx) = bounded(1);
//!
//! // Send a message into the channel.
//! tx.send("Hello, world!").unwrap();
//!
//! // Receive the message from the channel.
//! assert_eq!(rx.recv(), Ok("Hello, world!"));
//! ```
//!
//! # Creating way
//!
//! Channels can be created using the function:
//!
//! * [`bounded`] creates a channel of bounded capacity, i.e. there is a limit to how many messages
//!   it can hold at a time.
//!
//! The function returns a [`Sender`] and a [`Receiver`], which represent the two opposite sides
//! of a channel.
//!
//! ```
//! use omango::mpmc::bounded;
//!
//! // Create a channel that can hold at most 5 messages at a time.
//! let (tx, rx) = bounded(5);
//!
//! // Can send only 5 messages without blocking.
//! for i in 0..5 {
//!     tx.send(i).unwrap();
//! }
//!
//! // Another call to `send` would block because the channel is full.
//! // s.send(5).unwrap();
//! ```
//!
//! A special case is zero-capacity channel, which cannot hold any messages. Instead, send and
//! receive operations must appear at the same time in order to pair up and pass the message over:
//!
//! ```
//! use std::thread;
//! use omango::mpmc::bounded;
//!
//! // Create a zero-capacity channel.
//! let (tx, rx) = bounded(0);
//!
//! // Sending blocks until a receive operation appears on the other side.
//! thread::spawn(move || tx.send("Hi!").unwrap());
//!
//! // Receiving blocks until a send operation appears on the other side.
//! assert_eq!(rx.recv(), Ok("Hi!"));
//! ```
//!
//! # Sharing channels
//!
//! Senders and receivers can be cloned and sent to other threads:
//!
//! ```
//! use std::thread;
//! use omango::mpmc::bounded;
//!
//! let (tx1, rx1) = bounded(0);
//! let (tx2, rx2) = (tx1.clone(), rx1.clone());
//!
//! // Spawn a thread that receives a message and then sends one.
//! thread::spawn(move || {
//!     rx2.recv().unwrap();
//!     tx2.send(2).unwrap();
//! });
//!
//! // Send a message and then receive one.
//! tx1.send(1).unwrap();
//! rx1.recv().unwrap();
//! ```
//!
//! Note that cloning only creates a new handle to the same sending or receiving side. It does not
//! create a separate stream of messages in any way:
//!
//! ```
//! use omango::mpmc::bounded;
//!
//! let (tx1, rx1) = bounded(3);
//! let (tx2, rx2) = (tx1.clone(), rx1.clone());
//! let (tx3, rx3) = (tx2.clone(), rx2.clone());
//!
//! tx1.send(10).unwrap();
//! tx2.send(20).unwrap();
//! tx3.send(30).unwrap();
//!
//! assert_eq!(rx3.recv(), Ok(10));
//! assert_eq!(rx1.recv(), Ok(20));
//! assert_eq!(rx2.recv(), Ok(30));
//! ```
//!
//! # Disconnection
//!
//! When one of senders or one of receivers associated with a channel get closed, the channel becomes
//! disconnected. No more messages can be sent, but any remaining messages can still be received
//! by using [`try_recv`] until the channel is empty.
//! Then send and receive operations on a disconnected channel always fail.
//!
//! ```
//! use omango::mpmc::bounded;
//! use omango::error::{RecvError, TryRecvError};
//!
//! let (tx, rx) = bounded(3);
//! tx.send(1).unwrap();
//! tx.send(2).unwrap();
//! tx.send(3).unwrap();
//!
//! // Disconnects the channel.
//! tx.close();
//!
//! // This call doesn't block because the channel is now disconnected.
//! // `Err(RecvError)` is returned immediately.
//! assert_eq!(rx.recv(), Err(RecvError));
//!
//! // The remaining messages can be received.
//! assert_eq!(rx.try_recv(), Ok(1));
//! assert_eq!(rx.try_recv(), Ok(2));
//! assert_eq!(rx.try_recv(), Ok(3));
//! assert_eq!(rx.try_recv(), Err(TryRecvError));
//! ```
//!
//! # Blocking operations
//!
//! Send and receive operations come in three flavors:
//!
//! * Non-blocking (returns immediately with success or failure).
//! * Blocking (waits until the operation succeeds or the channel becomes disconnected).
//!
//! A simple example showing the difference between non-blocking and blocking operations:
//!
//! ```
//! use omango::mpmc::bounded;
//! use omango::error::{RecvError, TryRecvError};
//!
//! let (tx, rx) = bounded(1);
//!
//! // Send a message into the channel.
//! tx.send("foo").unwrap();
//!
//! // This call would block because the channel is full.
//! // s.send("bar").unwrap();
//!
//! // Receive the message.
//! assert_eq!(rx.recv(), Ok("foo"));
//!
//! // This call would block because the channel is empty.
//! // r.recv();
//!
//! // Try receiving a message without blocking.
//! assert_eq!(rx.try_recv(), Err(TryRecvError));
//!
//! // Disconnects the channel.
//! tx.close();
//!
//! // This call doesn't block because the channel is now disconnected.
//! assert_eq!(rx.recv(), Err(RecvError));
//! ```
//!
//! [`send`]: Sender::send
//! [`recv`]: Receiver::recv
//! [`try_recv`]: Receiver::try_recv

use std::cell::UnsafeCell;
use std::sync::Arc;

use crate::error::{RecvError, SendError, TryRecvError, TrySendError};
use crate::mpmc::queue::Mpmc;

mod waker;
mod queue;

/// Creates a channel of bounded capacity.
///
/// This channel has a buffer that can hold at most cap messages at a time.
///
/// A special case is zero-capacity channel, which cannot hold any messages.
/// Instead, send and receive operations must appear at the same time
/// in order to pair up and pass the message over:
///
/// # Examples
///
/// A channel of capacity 1:
///
/// ```
/// use std::thread;
/// use std::time::Duration;
/// use omango::mpmc::bounded;
///
/// let (tx, rx) = bounded(1);
///
/// // This call returns immediately because there is enough space in the channel.
/// tx.send(1).unwrap();
///
/// thread::spawn(move || {
///     // This call blocks the current thread because the channel is full.
///     // It will be able to complete only after the first message is received.
///     tx.send(2).unwrap();
/// });
///
/// thread::sleep(Duration::from_secs(1));
/// assert_eq!(rx.recv(), Ok(1));
/// assert_eq!(rx.recv(), Ok(2));
/// ```
///
/// A zero-capacity channel:
///
/// ```
/// use std::thread;
/// use std::time::Duration;
/// use omango::mpmc::bounded;
///
/// let (tx, rx) = bounded(0);
///
/// thread::spawn(move || {
///     // This call blocks the current thread until a receive operation appears
///     // on the other side of the channel.
///     tx.send(1).unwrap();
/// });
///
/// thread::sleep(Duration::from_secs(1));
/// assert_eq!(rx.recv(), Ok(1));
/// ```
#[inline]
pub fn bounded<T: Send>(size: u32) -> (Sender<T>, Receiver<T>) {
    let queue = Arc::new(UnsafeCell::new(Mpmc::new(size)));
    (Sender::new(queue.clone()), Receiver::new(queue))
}

/// The sending side of a channel.
///
/// # Examples
///
/// ```
/// use std::thread;
/// use omango::mpmc::bounded;
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
pub struct Sender<T> {
    inner: Arc<UnsafeCell<Mpmc<T>>>,
}

unsafe impl<T: Send> Send for Sender<T> {}

unsafe impl<T: Send> Sync for Sender<T> {}

impl<T: Send> Clone for Sender<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<T: Send> Sender<T> {
    #[inline]
    fn new(inner: Arc<UnsafeCell<Mpmc<T>>>) -> Self <> {
        Self { inner }
    }

    /// Attempts to send a message into the channel without blocking.
    ///
    /// This method will either send a message into the channel immediately or return an error if
    /// the channel is full or disconnected. The returned error contains the original message.
    ///
    /// # Examples
    ///
    /// ```
    /// use omango::mpmc::bounded;
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
    #[inline]
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        unsafe { (*self.inner.get()).try_send(value) }
    }

    /// Blocks the current thread until a message is sent or the channel is disconnected.
    ///
    /// If the channel is full and not disconnected, this call will block until the send operation
    /// can proceed. If the channel becomes disconnected, this call will wake up and return an
    /// error. The returned error contains the original message.
    ///
    /// If called on a zero-capacity channel, this method will wait for a receive operation to
    /// appear on the other side of the channel.
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    /// use omango::mpmc::bounded;
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
    #[inline]
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        unsafe { (*self.inner.get()).send(value) }
    }

    /// Fires closing channel notification.
    ///
    /// After closed, all [`try_send`], [`send`] and [`recv`] operations will be failed.
    ///
    /// Uses [`try_recv`] to read remaining messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    /// use omango::mpmc::bounded;
    /// use omango::error::{RecvError, SendError};
    ///
    /// let (tx, rx) = bounded(0);
    ///
    /// thread::spawn(move || {
    ///     thread::sleep(Duration::from_secs(1));
    ///     assert_eq!(tx.send(1), Ok(()));
    ///     tx.close();
    /// });
    ///
    /// assert_eq!(rx.recv(), Err(RecvError));
    /// assert_eq!(rx.try_recv(), Ok(1));
    /// ```
    ///
    /// [`send`]: Sender::send
    /// [`try_send`]: Sender::try_send
    /// [`recv`]: Receiver::recv
    /// [`try_recv`]: Receiver::try_recv
    #[inline]
    pub fn close(&self) {
        unsafe { (*self.inner.get()).close() }
    }
}

/// The receiving side of a channel.
///
/// # Examples
///
/// ```
/// use std::thread;
/// use std::time::Duration;
/// use omango::mpmc::bounded;
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
pub struct Receiver<T> {
    inner: Arc<UnsafeCell<Mpmc<T>>>,
}

unsafe impl<T: Send> Send for Receiver<T> {}

unsafe impl<T: Send> Sync for Receiver<T> {}

impl<T: Send> Clone for Receiver<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<T: Send> Receiver<T> {
    #[inline]
    fn new(inner: Arc<UnsafeCell<Mpmc<T>>>) -> Self <> {
        Self { inner }
    }

    /// Attempts to receive a message from the channel without blocking.
    ///
    /// This method will either receive a message from the channel immediately or return an error
    /// if the channel is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use omango::mpmc::bounded;
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
    #[inline]
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        unsafe { (*self.inner.get()).try_recv() }
    }

    /// Blocks the current thread until a message is received or the channel is empty and
    /// disconnected.
    ///
    /// If the channel is empty and not disconnected, this call will block until the receive
    /// operation can proceed. If the channel is empty and becomes disconnected, this call will
    /// wake up and return an error.
    ///
    /// If called on a zero-capacity channel, this method will wait for a send operation to appear
    /// on the other side of the channel.
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    /// use omango::mpmc::bounded;
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
    #[inline]
    pub fn recv(&self) -> Result<T, RecvError> {
        unsafe { (*self.inner.get()).recv() }
    }

    /// Fires closing channel notification.
    ///
    /// After closed, all [`try_send`], [`send`] and [`recv`] operations will be failed.
    ///
    /// Uses [`try_recv`] to read remaining messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    /// use std::time::Duration;
    /// use omango::mpmc::bounded;
    /// use omango::error::{RecvError, SendError};
    ///
    /// let (tx, rx) = bounded(0);
    ///
    /// thread::spawn(move || {
    ///     thread::sleep(Duration::from_secs(1));
    ///     assert_eq!(tx.send(1), Ok(()));
    ///     tx.close();
    /// });
    ///
    /// assert_eq!(rx.recv(), Err(RecvError));
    /// assert_eq!(rx.try_recv(), Ok(1));
    /// ```
    ///
    /// [`send`]: Sender::send
    /// [`try_send`]: Sender::try_send
    /// [`recv`]: Receiver::recv
    /// [`try_recv`]: Receiver::try_recv
    #[inline]
    pub fn close(&self) {
        unsafe { (*self.inner.get()).close() }
    }
}