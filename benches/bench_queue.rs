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

use std::thread;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use ringbuf::{
    HeapRb,
    traits::{Consumer, Producer, Split},
};

#[allow(unused)]
fn omango_spsc_block() {
    let (tx, rx) = omango::queue::spsc::bounded(1023);
    let thread = thread::spawn(move || {
        for _ in 0..2000 {
            assert_eq!(tx.send(1), Ok(()));
        }
    });
    for _ in 0..2000 {
        assert_eq!(rx.recv(), Ok(1));
    }
    thread.join().unwrap();
}

#[allow(unused)]
fn omango_spsc_nonblock() {
    let (tx, rx) = omango::queue::spsc::bounded(1023);
    let thread = thread::spawn(move || {
        for _ in 0..2000 {
            loop {
                match tx.try_send(1) {
                    Ok(()) => break,
                    Err(_) => continue
                }
            }
        }
    });
    for _ in 0..2000 {
        loop {
            match rx.try_recv() {
                Ok(v) => {
                    assert_eq!(v, 1);
                    break;
                }
                Err(_) => continue
            }
        }
    }
    thread.join().unwrap();
}

#[allow(unused)]
fn omango_spsc_unbounded() {
    let (tx, rx) = omango::queue::spsc::unbounded();
    let thread = thread::spawn(move || {
        for _ in 0..2000 {
            tx.send(1).unwrap();
        }
    });
    for _ in 0..2000 {
        assert_eq!(rx.recv(), Ok(1));
    }
    thread.join().unwrap();
}

#[allow(unused)]
fn omango_mpsc_block() {
    let (tx, rx) = omango::queue::mpmc::bounded(1023);
    let nthreads = (2 * num_cpus::get()) - 2;
    let mut sending_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                assert_eq!(txc.send(1), Ok(()));
            }
        });
        sending_threads.push(thread);
    }
    for _ in 0..nthreads * 1000 {
        assert_eq!(rx.recv(), Ok(1));
    }
    for thread in sending_threads {
        thread.join().expect("oops! the child thread panicked");
    }
}

#[allow(unused)]
fn omango_mpsc_nonblock() {
    let (tx, rx) = omango::queue::mpmc::bounded(1023);
    let nthreads = (2 * num_cpus::get()) - 2;
    let mut sending_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                loop {
                    match txc.try_send(1) {
                        Ok(()) => break,
                        Err(_) => {
                            core::hint::spin_loop();
                            continue;
                        }
                    }
                }
            }
        });
        sending_threads.push(thread);
    }
    for _ in 0..nthreads * 1000 {
        loop {
            match rx.try_recv() {
                Ok(v) => {
                    assert_eq!(v, 1);
                    break;
                }
                Err(_) => {
                    core::hint::spin_loop();
                    continue;
                }
            }
        }
    }
    for thread in sending_threads {
        thread.join().expect("oops! the child thread panicked");
    }
}

#[allow(unused)]
fn omango_mpmc_block() {
    let (tx, rx) = omango::queue::mpmc::bounded(1023);
    let nthreads = num_cpus::get() - 1;
    let mut sending_threads = Vec::new();
    let mut receiving_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                assert_eq!(txc.send(1), Ok(()));
            }
        });
        sending_threads.push(thread);
    }

    for _ in 0..nthreads {
        let rxc = rx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
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

#[allow(unused)]
fn omango_mpmc_nonblock() {
    let (tx, rx) = omango::queue::mpmc::bounded(1023);
    let nthreads = num_cpus::get() - 1;
    let mut sending_threads = Vec::new();
    let mut receiving_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                loop {
                    match txc.try_send(1) {
                        Ok(()) => break,
                        Err(_) => {
                            core::hint::spin_loop();
                            continue;
                        }
                    }
                }
            }
        });
        sending_threads.push(thread);
    }

    for _ in 0..nthreads {
        let rxc = rx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                loop {
                    match rxc.try_recv() {
                        Ok(v) => {
                            assert_eq!(v, 1);
                            break;
                        }
                        Err(_) => {
                            core::hint::spin_loop();
                            continue;
                        }
                    }
                }
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

#[allow(unused)]
fn omango_mpmc_unbounded() {
    let (tx, rx) = omango::queue::mpmc::unbounded();
    let nthreads = num_cpus::get() - 1;
    let mut sending_threads = Vec::new();
    let mut receiving_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                assert_eq!(txc.send(1), Ok(()));
            }
        });
        sending_threads.push(thread);
    }

    for _ in 0..nthreads {
        let rxc = rx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
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

#[allow(unused)]
fn std_spsc_block() {
    let (tx, rx) = std::sync::mpsc::sync_channel(1024);
    let thread = thread::spawn(move || {
        for _ in 0..2000 {
            assert_eq!(tx.send(1), Ok(()));
        }
    });
    for _ in 0..2000 {
        assert_eq!(rx.recv(), Ok(1));
    }
    thread.join().unwrap();
}

#[allow(unused)]
fn std_spsc_noblock() {
    let (tx, rx) = std::sync::mpsc::sync_channel(1024);
    let thread = thread::spawn(move || {
        for _ in 0..2000 {
            loop {
                match tx.try_send(1) {
                    Ok(()) => break,
                    Err(_) => continue
                }
            }
        }
    });
    for _ in 0..2000 {
        loop {
            match rx.try_recv() {
                Ok(v) => {
                    assert_eq!(v, 1);
                    break;
                }
                Err(_) => continue
            }
        }
    }
    thread.join().unwrap();
}

#[allow(unused)]
fn std_mpsc_block() {
    let (tx, rx) = std::sync::mpsc::sync_channel(1024);
    let nthreads = (2 * num_cpus::get()) - 2;
    let mut sending_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                assert_eq!(txc.send(1), Ok(()));
            }
        });
        sending_threads.push(thread);
    }
    for _ in 0..nthreads * 1000 {
        assert_eq!(rx.recv(), Ok(1));
    }
    for thread in sending_threads {
        thread.join().expect("oops! the child thread panicked");
    }
}

#[allow(unused)]
fn std_mpsc_nonblock() {
    let (tx, rx) = std::sync::mpsc::sync_channel(1024);
    let nthreads = (2 * num_cpus::get()) - 2;
    let mut sending_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                loop {
                    match txc.try_send(1) {
                        Ok(()) => break,
                        Err(_) => {
                            core::hint::spin_loop();
                            continue;
                        }
                    }
                }
            }
        });
        sending_threads.push(thread);
    }
    for _ in 0..nthreads * 1000 {
        loop {
            match rx.try_recv() {
                Ok(v) => {
                    assert_eq!(v, 1);
                    break;
                }
                Err(_) => {
                    core::hint::spin_loop();
                    continue;
                }
            }
        }
    }
    for thread in sending_threads {
        thread.join().expect("oops! the child thread panicked");
    }
}

#[allow(unused)]
fn flume_spsc_block() {
    let (tx, rx) = flume::bounded(1024);
    let thread = thread::spawn(move || {
        for _ in 1..2000 {
            assert_eq!(tx.send(1), Ok(()));
        }
    });
    for _ in 1..2000 {
        assert_eq!(rx.recv(), Ok(1));
    }
    thread.join().unwrap();
}

#[allow(unused)]
fn flume_spsc_nonblock() {
    let (tx, rx) = flume::bounded(1024);
    let thread = thread::spawn(move || {
        for _ in 0..2000 {
            loop {
                match tx.try_send(1) {
                    Ok(()) => break,
                    Err(_) => continue
                }
            }
        }
    });
    for _ in 0..2000 {
        loop {
            match rx.try_recv() {
                Ok(v) => {
                    assert_eq!(v, 1);
                    break;
                }
                Err(_) => continue
            }
        }
    }
    thread.join().unwrap();
}

#[allow(unused)]
fn flume_mpsc_block() {
    let (tx, rx) = flume::bounded(1024);
    let nthreads = (2 * num_cpus::get()) - 2;
    let mut sending_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                assert_eq!(txc.send(1), Ok(()));
            }
        });
        sending_threads.push(thread);
    }
    for _ in 0..nthreads * 1000 {
        assert_eq!(rx.recv(), Ok(1));
    }
    for thread in sending_threads {
        thread.join().expect("oops! the child thread panicked");
    }
}

#[allow(unused)]
fn flume_mpsc_nonblock() {
    let (tx, rx) = flume::bounded(1024);
    let nthreads = (2 * num_cpus::get()) - 2;
    let mut sending_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                loop {
                    match txc.try_send(1) {
                        Ok(()) => break,
                        Err(_) => {
                            core::hint::spin_loop();
                            continue;
                        }
                    }
                }
            }
        });
        sending_threads.push(thread);
    }
    for _ in 0..nthreads * 1000 {
        loop {
            match rx.try_recv() {
                Ok(v) => {
                    assert_eq!(v, 1);
                    break;
                }
                Err(_) => {
                    core::hint::spin_loop();
                    continue;
                }
            }
        }
    }
    for thread in sending_threads {
        thread.join().expect("oops! the child thread panicked");
    }
}

#[allow(unused)]
fn flume_mpmc_block() {
    let (tx, rx) = flume::bounded(1024);
    let nthreads = num_cpus::get() - 1;
    let mut sending_threads = Vec::new();
    let mut receiving_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                assert_eq!(txc.send(1), Ok(()));
            }
        });
        sending_threads.push(thread);
    }

    for _ in 0..nthreads {
        let rxc = rx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
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

#[allow(unused)]
fn flume_mpmc_nonblock() {
    let (tx, rx) = flume::bounded(1024);
    let nthreads = num_cpus::get() - 1;
    let mut sending_threads = Vec::new();
    let mut receiving_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                loop {
                    match txc.try_send(1) {
                        Ok(()) => break,
                        Err(_) => {
                            core::hint::spin_loop();
                            continue;
                        }
                    }
                }
            }
        });
        sending_threads.push(thread);
    }

    for _ in 0..nthreads {
        let rxc = rx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                loop {
                    match rxc.try_recv() {
                        Ok(v) => {
                            assert_eq!(v, 1);
                            break;
                        }
                        Err(_) => {
                            core::hint::spin_loop();
                            continue;
                        }
                    }
                }
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

#[allow(unused)]
fn crossbeam_spsc_block() {
    let (tx, rx) = crossbeam_channel::bounded(1023);
    let thread = thread::spawn(move || {
        for _ in 1..2000 {
            assert_eq!(tx.send(1), Ok(()));
        }
    });
    for _ in 1..2000 {
        assert_eq!(rx.recv(), Ok(1));
    }
    thread.join().unwrap();
}

#[allow(unused)]
fn crossbeam_spsc_nonblock() {
    let (tx, rx) = crossbeam_channel::bounded(1023);
    let thread = thread::spawn(move || {
        for _ in 0..2000 {
            loop {
                match tx.try_send(1) {
                    Ok(()) => break,
                    Err(_) => continue
                }
            }
        }
    });
    for _ in 0..2000 {
        loop {
            match rx.try_recv() {
                Ok(v) => {
                    assert_eq!(v, 1);
                    break;
                }
                Err(_) => continue
            }
        }
    }
    thread.join().unwrap();
}

#[allow(unused)]
fn crossbeam_spsc_unbounded() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let thread = thread::spawn(move || {
        for _ in 1..2000 {
            assert_eq!(tx.send(1), Ok(()));
        }
    });
    for _ in 1..2000 {
        assert_eq!(rx.recv(), Ok(1));
    }
    thread.join().unwrap();
}

#[allow(unused)]
fn crossbeam_mpsc_block() {
    let (tx, rx) = crossbeam_channel::bounded(1023);
    let nthreads = (2 * num_cpus::get()) - 2;
    let mut sending_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                assert_eq!(txc.send(1), Ok(()));
            }
        });
        sending_threads.push(thread);
    }
    for _ in 0..nthreads * 1000 {
        assert_eq!(rx.recv(), Ok(1));
    }
    for thread in sending_threads {
        thread.join().expect("oops! the child thread panicked");
    }
}

#[allow(unused)]
fn crossbeam_mpsc_nonblock() {
    let (tx, rx) = crossbeam_channel::bounded(1023);
    let nthreads = (2 * num_cpus::get()) - 2;
    let mut sending_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                loop {
                    match txc.try_send(1) {
                        Ok(()) => break,
                        Err(_) => {
                            core::hint::spin_loop();
                            continue;
                        }
                    }
                }
            }
        });
        sending_threads.push(thread);
    }
    for _ in 0..nthreads * 1000 {
        loop {
            match rx.try_recv() {
                Ok(v) => {
                    assert_eq!(v, 1);
                    break;
                }
                Err(_) => {
                    core::hint::spin_loop();
                    continue;
                }
            }
        }
    }
    for thread in sending_threads {
        thread.join().expect("oops! the child thread panicked");
    }
}

#[allow(unused)]
fn crossbeam_mpmc_block() {
    let (tx, rx) = crossbeam_channel::bounded(1023);
    let nthreads = num_cpus::get() - 1;
    let mut sending_threads = Vec::new();
    let mut receiving_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                assert_eq!(txc.send(1), Ok(()));
            }
        });
        sending_threads.push(thread);
    }

    for _ in 0..nthreads {
        let rxc = rx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
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

#[allow(unused)]
fn crossbeam_mpmc_nonblock() {
    let (tx, rx) = crossbeam_channel::bounded(1023);
    let nthreads = num_cpus::get() - 1;
    let mut sending_threads = Vec::new();
    let mut receiving_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                loop {
                    match txc.try_send(1) {
                        Ok(()) => break,
                        Err(_) => {
                            core::hint::spin_loop();
                            continue;
                        }
                    }
                }
            }
        });
        sending_threads.push(thread);
    }

    for _ in 0..nthreads {
        let rxc = rx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                loop {
                    match rxc.try_recv() {
                        Ok(v) => {
                            assert_eq!(v, 1);
                            break;
                        }
                        Err(_) => {
                            core::hint::spin_loop();
                            continue;
                        }
                    }
                }
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

#[allow(unused)]
fn crossbeam_mpmc_unbounded() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let nthreads = num_cpus::get() - 1;
    let mut sending_threads = Vec::new();
    let mut receiving_threads = Vec::new();

    for _ in 0..nthreads {
        let txc = tx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
                assert_eq!(txc.send(1), Ok(()));
            }
        });
        sending_threads.push(thread);
    }

    for _ in 0..nthreads {
        let rxc = rx.clone();
        let thread = thread::spawn(move || {
            for _ in 0..1000 {
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

#[allow(unused)]
fn rtrb_spsc_nonblock() {
    let (mut tx, mut rx) = rtrb::RingBuffer::new(1024);
    let thread = thread::spawn(move || {
        for _ in 1..2000 {
            loop {
                match tx.push(1) {
                    Ok(()) => break,
                    Err(_) => continue
                }
            }
        }
    });
    for _ in 1..2000 {
        loop {
            match rx.pop() {
                Ok(v) => {
                    assert_eq!(v, 1);
                    break;
                }
                Err(_) => continue
            }
        }
    }
    thread.join().unwrap();
}

#[allow(unused)]
fn ringbuf_spsc_nonblock() {
    let rb = HeapRb::<i32>::new(1024);
    let (mut tx, mut rx) = rb.split();
    let thread = thread::spawn(move || {
        for _ in 1..2000 {
            loop {
                match tx.try_push(1) {
                    Ok(()) => break,
                    Err(_) => continue
                }
            }
        }
    });
    for _ in 1..2000 {
        loop {
            match rx.try_pop() {
                Some(v) => {
                    assert_eq!(v, 1);
                    break;
                }
                None => continue
            }
        }
    }
    thread.join().unwrap();
}

#[allow(unused)]
fn bench_spsc_block(c: &mut Criterion) {
    let mut group = c.benchmark_group("SPSC-Block");
    for i in 0u64..3u64 {
        group.bench_function(BenchmarkId::new("Standard", i),
                             |b| b.iter(std_spsc_block));
        group.bench_function(BenchmarkId::new("Flume", i),
                             |b| b.iter(flume_spsc_block));
        group.bench_function(BenchmarkId::new("Crossbeam", i),
                             |b| b.iter(crossbeam_spsc_block));
        group.bench_function(BenchmarkId::new("Omango", i),
                             |b| b.iter(omango_spsc_block));
    }
    group.finish();
}

#[allow(unused)]
fn bench_spsc_nonblock(c: &mut Criterion) {
    let mut group = c.benchmark_group("SPSC-Nonblock");
    for i in 0u64..3u64 {
        group.bench_function(BenchmarkId::new("Standard", i),
                             |b| b.iter(std_spsc_block));
        group.bench_function(BenchmarkId::new("Flume", i),
                             |b| b.iter(flume_spsc_block));
        group.bench_function(BenchmarkId::new("Crossbeam", i),
                             |b| b.iter(crossbeam_spsc_block));
        group.bench_function(BenchmarkId::new("Rtrb", i),
                             |b| b.iter(rtrb_spsc_nonblock));
        group.bench_function(BenchmarkId::new("Ringbuf", i),
                             |b| b.iter(ringbuf_spsc_nonblock));
        group.bench_function(BenchmarkId::new("Omango", i),
                             |b| b.iter(omango_spsc_nonblock));
    }
    group.finish();
}

#[allow(unused)]
fn bench_spsc_unbounded(c: &mut Criterion) {
    let mut group = c.benchmark_group("SPSC-Unbounded");
    for i in 0u64..3u64 {
        group.bench_function(BenchmarkId::new("Omango-unbounded", i),
                             |b| b.iter(omango_spsc_unbounded));
        group.bench_function(BenchmarkId::new("Crossbeam-unbounded", i),
                             |b| b.iter(crossbeam_spsc_unbounded));
    }
    group.finish();
}

#[allow(unused)]
fn bench_mpsc_block(c: &mut Criterion) {
    let mut group = c.benchmark_group("MPSC-Block");
    for i in 0u64..3u64 {
        group.bench_function(BenchmarkId::new("Standard", i),
                             |b| b.iter(std_mpsc_block));
        group.bench_function(BenchmarkId::new("Flume", i),
                             |b| b.iter(flume_mpsc_block));
        group.bench_function(BenchmarkId::new("Crossbeam", i),
                             |b| b.iter(crossbeam_mpsc_block));
        group.bench_function(BenchmarkId::new("Omango", i),
                             |b| b.iter(omango_mpsc_block));
    }
    group.finish();
}

#[allow(unused)]
fn bench_mpsc_nonblock(c: &mut Criterion) {
    let mut group = c.benchmark_group("MPSC-Nonblock");
    for i in 0u64..3u64 {
        group.bench_function(BenchmarkId::new("Standard", i),
                             |b| b.iter(std_mpsc_nonblock));
        group.bench_function(BenchmarkId::new("Flume", i),
                             |b| b.iter(flume_mpsc_nonblock));
        group.bench_function(BenchmarkId::new("Crossbeam", i),
                             |b| b.iter(crossbeam_mpsc_nonblock));
        group.bench_function(BenchmarkId::new("Omango", i),
                             |b| b.iter(omango_mpsc_nonblock));
    }
    group.finish();
}

#[allow(unused)]
fn bench_mpmc_block(c: &mut Criterion) {
    let mut group = c.benchmark_group("MPMC-Block");
    for i in 0..3u64 {
        group.bench_function(BenchmarkId::new("Flume", i),
                             |b| b.iter(flume_mpmc_block));
        group.bench_function(BenchmarkId::new("Crossbeam", i),
                             |b| b.iter(crossbeam_mpmc_block));
        group.bench_function(BenchmarkId::new("Omango", i),
                             |b| b.iter(omango_mpmc_block));
    }
    group.finish();
}

#[allow(unused)]
fn bench_mpmc_nonblock(c: &mut Criterion) {
    let mut group = c.benchmark_group("MPMC-Nonblock");
    for i in 0..3u64 {
        group.bench_function(BenchmarkId::new("Flume", i),
                             |b| b.iter(flume_mpmc_nonblock));
        group.bench_function(BenchmarkId::new("Crossbeam", i),
                             |b| b.iter(crossbeam_mpmc_nonblock));
        group.bench_function(BenchmarkId::new("Omango", i),
                             |b| b.iter(omango_mpmc_nonblock));
    }
    group.finish();
}

#[allow(unused)]
fn bench_mpmc_unbounded(c: &mut Criterion) {
    let mut group = c.benchmark_group("MPMC-Unbounded");
    for i in 0..3u64 {
        group.bench_function(BenchmarkId::new("Crossbeam", i),
                             |b| b.iter(crossbeam_mpmc_unbounded));
        group.bench_function(BenchmarkId::new("Omango", i),
                             |b| b.iter(omango_mpmc_unbounded));
    }
    group.finish();
}

criterion_group!(benches, bench_mpmc_block);
criterion_main!(benches);