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

extern crate core;

use std::thread;
use std::time::Duration;

fn is_send<T: Send>() {}

#[test]
fn bounds() {
    // SPSC
    is_send::<omango::spsc::Sender<i32>>();
    is_send::<omango::spsc::Receiver<i32>>();

    // MPMC
    is_send::<omango::mpmc::Sender<i32>>();
    is_send::<omango::mpmc::Receiver<i32>>();
}

#[test]
fn send_recv() {
    // SPSC
    {
        let (tx, rx) = omango::spsc::bounded(4);
        tx.send(1).unwrap();
        assert_eq!(rx.recv().unwrap(), 1);
    }

    // MPMC
    let (tx, rx) = omango::mpmc::bounded(4);
    tx.send(1).unwrap();
    assert_eq!(rx.recv().unwrap(), 1);
}

#[test]
fn send_shared_recv() {
    // SPSC
    {
        let (tx1, rx) = omango::spsc::bounded(4);
        let tx2 = tx1.clone();

        tx1.send(1).unwrap();
        assert_eq!(rx.recv().unwrap(), 1);

        tx2.send(2).unwrap();
        assert_eq!(rx.recv().unwrap(), 2);
    }

    // MPMC
    let (tx1, rx) = omango::mpmc::bounded(4);
    let tx2 = tx1.clone();

    tx1.send(1).unwrap();
    assert_eq!(rx.recv().unwrap(), 1);

    tx2.send(2).unwrap();
    assert_eq!(rx.recv().unwrap(), 2);
}

#[test]
fn send_recv_threads() {
    // SPSC
    {
        let (tx, rx) = omango::spsc::bounded(4);
        let thread = thread::spawn(move || {
            tx.send(1).unwrap();
        });
        assert_eq!(rx.recv().unwrap(), 1);
        thread.join().unwrap();
    }

    // MPMC
    let (tx, rx) = omango::mpmc::bounded(4);
    let thread = thread::spawn(move || {
        tx.send(1).unwrap();
    });
    assert_eq!(rx.recv().unwrap(), 1);
    thread.join().unwrap();
}

#[test]
fn send_recv_threads_no_capacity() {
    // SPSC
    {
        let (tx, rx) = omango::spsc::bounded(0);
        let thread = thread::spawn(move || {
            tx.send(1).unwrap();
            tx.send(2).unwrap();
        });

        thread::sleep(Duration::from_millis(100));
        assert_eq!(rx.recv().unwrap(), 1);

        thread::sleep(Duration::from_millis(100));
        assert_eq!(rx.recv().unwrap(), 2);

        thread.join().unwrap();
    }

    // MPMC
    let (tx, rx) = omango::mpmc::bounded(0);
    let thread = thread::spawn(move || {
        tx.send(1).unwrap();
        tx.send(2).unwrap();
    });

    thread::sleep(Duration::from_millis(100));
    assert_eq!(rx.recv().unwrap(), 1);

    thread::sleep(Duration::from_millis(100));
    assert_eq!(rx.recv().unwrap(), 2);

    thread.join().unwrap();
}

#[test]
fn send_close_gets_none() {
    // SPSC
    {
        let (tx, rx) = omango::spsc::bounded::<i32>(0);
        let thread = thread::spawn(move || {
            assert!(rx.recv().is_err());
        });
        tx.close();
        thread.join().unwrap();
    }

    // MPMC
    let (tx, rx) = omango::mpmc::bounded::<i32>(0);
    let thread = thread::spawn(move || {
        assert!(rx.recv().is_err());
    });
    tx.close();
    thread.join().unwrap();
}

#[test]
fn spsc_no_capacity() {
    let amt = 10000;
    let (tx, rx) = omango::spsc::bounded(0);

    let txc = tx.clone();
    thread::spawn(move || {
        for _ in 0..amt {
            assert_eq!(txc.send(1), Ok(()));
        }
    });
    for _ in 0..amt {
        assert_eq!(rx.recv(), Ok(1));
    }
}

#[test]
fn mpsc_no_capacity() {
    let amt = 10000;
    let nthreads = (2 * num_cpus::get()) - 1;
    let (tx, rx) = omango::mpmc::bounded(0);

    for _ in 0..nthreads {
        let txc = tx.clone();
        thread::spawn(move || {
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
    let amt = 10000;
    let nthreads_send = num_cpus::get() - 1;
    let nthreads_recv = num_cpus::get() - 1;
    let (tx, rx) = omango::mpmc::bounded(0);
    let mut receiving_threads = Vec::new();
    let mut sending_threads = Vec::new();

    for _ in 0..nthreads_send {
        let txc = tx.clone();
        let child = thread::spawn(move || {
            for _ in 0..amt {
                assert_eq!(txc.send(1), Ok(()));
            }
        });
        sending_threads.push(child);
    }

    for _ in 0..nthreads_recv {
        let rxc = rx.clone();
        let thread = thread::spawn(move || {
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