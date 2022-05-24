use std::thread;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rtrb::RingBuffer;

fn my_spsc_block() {
    let (tx, rx) = omango::spsc::bounded(1024);
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

fn my_spsc_nonblock() {
    let (tx, rx) = omango::spsc::bounded(1024);
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

fn my_mpsc_block() {
    let (tx, rx) = omango::mpmc::bounded(1024);
    let nthreads = (2 * num_cpus::get()) - 1;
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

fn my_mpsc_nonblock() {
    let (tx, rx) = omango::mpmc::bounded(1024);
    let nthreads = (2 * num_cpus::get()) - 1;
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

fn my_mpmc_block() {
    let (tx, rx) = omango::mpmc::bounded(1024);
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

fn my_mpmc_nonblock() {
    let (tx, rx) = omango::mpmc::bounded(1024);
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

fn std_mpsc_block() {
    let (tx, rx) = std::sync::mpsc::sync_channel(1024);
    let nthreads = (2 * num_cpus::get()) - 1;
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

fn std_mpsc_nonblock() {
    let (tx, rx) = std::sync::mpsc::sync_channel(1024);
    let nthreads = (2 * num_cpus::get()) - 1;
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

fn flume_mpsc_block() {
    let (tx, rx) = flume::bounded(1024);
    let nthreads = (2 * num_cpus::get()) - 1;
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

fn flume_mpsc_nonblock() {
    let (tx, rx) = flume::bounded(1024);
    let nthreads = (2 * num_cpus::get()) - 1;
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

fn crossbeam_spsc_block() {
    let (tx, rx) = crossbeam_channel::bounded(1024);
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

fn crossbeam_spsc_nonblock() {
    let (tx, rx) = crossbeam_channel::bounded(1024);
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

fn crossbeam_mpsc_block() {
    let (tx, rx) = crossbeam_channel::bounded(1024);
    let nthreads = (2 * num_cpus::get()) - 1;
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

fn crossbeam_mpsc_nonblock() {
    let (tx, rx) = crossbeam_channel::bounded(1024);
    let nthreads = (2 * num_cpus::get()) - 1;
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

fn crossbeam_mpmc_block() {
    let (tx, rx) = crossbeam_channel::bounded(1024);
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

fn crossbeam_mpmc_nonblock() {
    let (tx, rx) = crossbeam_channel::bounded(1024);
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

fn rtrb_spsc_nonblock() {
    let (mut tx, mut rx) = RingBuffer::new(1024);
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

fn bench_spsc_block(c: &mut Criterion) {
    let mut group = c.benchmark_group("SPSC-Block");
    for i in 0u64..3u64 {
        group.bench_function(BenchmarkId::new("Standard", &i),
                             |b| b.iter(|| std_spsc_block()));
        group.bench_function(BenchmarkId::new("Flume", &i),
                             |b| b.iter(|| flume_spsc_block()));
        group.bench_function(BenchmarkId::new("Crossbeam", &i),
                             |b| b.iter(|| crossbeam_spsc_block()));
        group.bench_function(BenchmarkId::new("My", &i),
                             |b| b.iter(|| my_spsc_block()));
    }
    group.finish();
}

fn bench_spsc_nonblock(c: &mut Criterion) {
    let mut group = c.benchmark_group("SPSC-Nonblock");
    for i in 0u64..3u64 {
        group.bench_function(BenchmarkId::new("Standard", &i),
                             |b| b.iter(|| std_spsc_block()));
        group.bench_function(BenchmarkId::new("Flume", &i),
                             |b| b.iter(|| flume_spsc_block()));
        group.bench_function(BenchmarkId::new("Crossbeam", &i),
                             |b| b.iter(|| crossbeam_spsc_block()));
        group.bench_function(BenchmarkId::new("Rtrb", &i),
                             |b| b.iter(|| rtrb_spsc_nonblock()));
        group.bench_function(BenchmarkId::new("My", &i),
                             |b| b.iter(|| my_spsc_block()));
    }
    group.finish();
}

fn bench_mpsc_block(c: &mut Criterion) {
    let mut group = c.benchmark_group("MPSC-Block");
    for i in 0u64..3u64 {
        group.bench_function(BenchmarkId::new("Standard", &i),
                             |b| b.iter(|| std_mpsc_block()));
        group.bench_function(BenchmarkId::new("Flume", &i),
                             |b| b.iter(|| flume_mpsc_block()));
        group.bench_function(BenchmarkId::new("Crossbeam", &i),
                             |b| b.iter(|| crossbeam_mpsc_block()));
        group.bench_function(BenchmarkId::new("My", &i),
                             |b| b.iter(|| my_mpsc_block()));
    }
    group.finish();
}

fn bench_mpsc_nonblock(c: &mut Criterion) {
    let mut group = c.benchmark_group("MPSC-Nonblock");
    for i in 0u64..3u64 {
        group.bench_function(BenchmarkId::new("Standard", &i),
                             |b| b.iter(|| std_mpsc_nonblock()));
        group.bench_function(BenchmarkId::new("Flume", &i),
                             |b| b.iter(|| flume_mpsc_nonblock()));
        group.bench_function(BenchmarkId::new("Crossbeam", &i),
                             |b| b.iter(|| crossbeam_mpsc_nonblock()));
        group.bench_function(BenchmarkId::new("My", &i),
                             |b| b.iter(|| my_mpsc_nonblock()));
    }
    group.finish();
}

fn bench_mpmc_block(c: &mut Criterion) {
    let mut group = c.benchmark_group("MPMC-Block");
    for i in 0..3u64 {
        group.bench_function(BenchmarkId::new("Flume", &i),
                             |b| b.iter(|| flume_mpmc_block()));
        group.bench_function(BenchmarkId::new("Crossbeam", &i),
                             |b| b.iter(|| crossbeam_mpmc_block()));
        group.bench_function(BenchmarkId::new("My", &i),
                             |b| b.iter(|| my_mpmc_block()));
    }
    group.finish();
}

fn bench_mpmc_nonblock(c: &mut Criterion) {
    let mut group = c.benchmark_group("MPMC-Nonblock");
    for i in 0..3u64 {
        group.bench_function(BenchmarkId::new("Flume", &i),
                             |b| b.iter(|| flume_mpmc_nonblock()));
        group.bench_function(BenchmarkId::new("Crossbeam", &i),
                             |b| b.iter(|| crossbeam_mpmc_nonblock()));
        group.bench_function(BenchmarkId::new("My", &i),
                             |b| b.iter(|| my_mpmc_nonblock()));
    }
    group.finish();
}

criterion_group!(benches, bench_spsc_nonblock);
criterion_main!(benches);