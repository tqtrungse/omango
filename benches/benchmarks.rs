use std::thread;
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

fn my_spsc() {
    let (tx, rx) = omango::spsc::bounded(1024);
    let thread = thread::spawn(move|| {
        for _ in 0..2000 {
            assert_eq!(tx.send(1), Ok(()));
        }
    });
    for _ in 0..2000 {
        assert_eq!(rx.recv(), Ok(1));
    }
    thread.join().unwrap();
}

fn my_mpsc() {
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

fn my_mpmc() {
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

fn std_spsc() {
    let (tx, rx) = std::sync::mpsc::sync_channel(1024);
    let thread = thread::spawn(move|| {
        for _ in 0..2000 {
            assert_eq!(tx.send(1), Ok(()));
        }
    });
    for _ in 0..2000 {
        assert_eq!(rx.recv(), Ok(1));
    }
    thread.join().unwrap();
}

fn std_mpsc() {
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

fn flume_spsc() {
    let (tx, rx) = flume::bounded(1024);
    let thread = thread::spawn(move|| {
        for _ in 1..2000 {
            assert_eq!(tx.send(1), Ok(()));
        }
    });
    for _ in 1..2000 {
        assert_eq!(rx.recv(), Ok(1));
    }
    thread.join().unwrap();
}

fn flume_mpsc() {
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

fn flume_mpmc() {
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

fn crossbeam_spsc() {
    let (tx, rx) = crossbeam_channel::bounded(1024);
    let thread = thread::spawn(move|| {
        for _ in 1..2000 {
            assert_eq!(tx.send(1), Ok(()));
        }
    });
    for _ in 1..2000 {
        assert_eq!(rx.recv(), Ok(1));
    }
    thread.join().unwrap();
}

fn crossbeam_mpsc() {
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

fn crossbeam_mpmc() {
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

fn bench_spsc(c: &mut Criterion) {
    let mut group = c.benchmark_group("SPSC");
    for i in 0u64..3u64 {
        group.bench_function(BenchmarkId::new("Standard", &i),
                             |b| b.iter(|| std_spsc()));
        group.bench_function(BenchmarkId::new("Flume", &i),
                             |b| b.iter(|| flume_spsc()));
        group.bench_function(BenchmarkId::new("Crossbeam", &i),
                             |b| b.iter(|| crossbeam_spsc()));
        group.bench_function(BenchmarkId::new("My", &i),
                             |b| b.iter(|| my_spsc()));
    }
    group.finish();
}

fn bench_mpsc(c: &mut Criterion) {
    let mut group = c.benchmark_group("MPSC");
    for i in 0u64..3u64 {
        group.bench_function(BenchmarkId::new("Standard", &i),
                             |b| b.iter(|| std_mpsc()));
        group.bench_function(BenchmarkId::new("Flume", &i),
                             |b| b.iter(|| flume_mpsc()));
        group.bench_function(BenchmarkId::new("Crossbeam", &i),
                             |b| b.iter(|| crossbeam_mpsc()));
        group.bench_function(BenchmarkId::new("My", &i),
                             |b| b.iter(|| my_mpsc()));
    }
    group.finish();
}

fn bench_mpmc(c: &mut Criterion) {
    let mut group = c.benchmark_group("MPMC");
    for i in 0..3u64 {
        group.bench_function(BenchmarkId::new("Flume", &i),
                             |b| b.iter(|| flume_mpmc()));
        group.bench_function(BenchmarkId::new("Crossbeam", &i),
                             |b| b.iter(|| crossbeam_mpmc()));
        group.bench_function(BenchmarkId::new("My", &i),
                             |b| b.iter(|| my_mpmc()));
    }
    group.finish();
}

criterion_group!(benches, bench_mpsc);
criterion_main!(benches);