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

//! Defines all errors is used in this crate.

use std::{error, fmt};

/// An error returned from the `send` method.
///
/// The message could not be sent because the channel is disconnected.
///
/// The error contains the message so it can be recovered.
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct SendError<T>(pub T);

/// An error returned from the `try_send` method.
///
/// The error contains the message being sent so it can be recovered.
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TrySendError<T> {
    /// The message could not be sent because the channel is full.
    ///
    /// If this is a zero-capacity channel, then the error indicates that there was no receiver
    /// available to receive the message at the time.
    Full(T),

    /// The message could not be sent because the channel is disconnected.
    Disconnected(T),
}

/// An error returned from the `recv` method.
///
/// A message could not be received because the channel is empty and disconnected.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct RecvError;

/// An error returned from the `try_recv` method.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct TryRecvError;

impl<T> fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "SendError(..)".fmt(f)
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "sending on a disconnected channel".fmt(f)
    }
}

impl<T> fmt::Debug for TrySendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            TrySendError::Full(..) => "Full(..)".fmt(f),
            TrySendError::Disconnected(..) => "Disconnected(..)".fmt(f),
        }
    }
}

impl<T> fmt::Display for TrySendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            TrySendError::Full(..) => "sending on a full channel".fmt(f),
            TrySendError::Disconnected(..) => "sending on a disconnected channel".fmt(f),
        }
    }
}

impl<T: Send> error::Error for TrySendError<T> {}

impl<T> TrySendError<T> {
    /// Unwraps the message.
    ///
    /// # Examples
    ///
    /// ```
    /// use omango::mpmc::bounded;
    ///
    /// let (tx, rx) = bounded(0);
    ///
    /// if let Err(err) = tx.try_send("foo") {
    ///     assert_eq!(err.into_inner(), "foo");
    /// }
    /// ```
    pub fn into_inner(self) -> T {
        match self {
            TrySendError::Full(v) => v,
            TrySendError::Disconnected(v) => v,
        }
    }
}

impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "receiving on an empty and disconnected channel".fmt(f)
    }
}

impl error::Error for RecvError {}

impl fmt::Display for TryRecvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "try receiving on an empty and disconnected channel".fmt(f)
    }
}

impl error::Error for TryRecvError {}