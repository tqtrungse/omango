<p align="center">
  <a href='https://postimg.cc/1gBgL2MX' target='_blank'>
    <img src='https://i.postimg.cc/1gBgL2MX/Pngtree-omango-in-flat-style-omango-3626110.png' border='0' alt='Pngtree-omango-in-flat-style-omango-3626110'/>
  </a>
</p>
<br/>
<p align="center">
  <a href="https://github.com/tqtrungse/omango/actions/workflows/rust.yml"><img src="https://github.com/tqtrungse/omango/actions/workflows/rust.yml/badge.svg?branch=master" alt="Rust"></a>
  <a href="https://opensource.org/licenses/MIT"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT"></a>
  <a href="https://github.com/tqtrungse/omango"><img src="https://img.shields.io/github/v/release/tqtrungse/omango" alt="Release"></a>
  <a href="https://crates.io/crates/omango"><img src="https://img.shields.io/crates/v/omango.svg" alt="Cargo"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-1.49+-lightgray.svg" alt="Rust 1.49+"></a>
</p>
<br/>

# Omango

This is a concurrency library.<br />

The crate provides a lock-free bounded single-producer-single-consumer channel and
multi-producer-multi-consumer channel.<br />

The channels are simple, lightweight, fast and safe in multithreading environment.
It is faster than [std::mpsc::sync_channel](https://github.com/rust-lang/rust/tree/master/library/std/src/sync/mpsc) 
and other open source's bounded queue ([multiqueue](https://github.com/schets/multiqueue), [flume](https://github.com/zesterer/flume), [crossbeam-channel](https://github.com/crossbeam-rs/crossbeam/tree/master/crossbeam-channel)). <br/>

## Table of Contents

- [Introduction](#introduction)
- [Usage](#usage)
- [Compatibility](#compatibility)
- [Benchmarks](#benchmarks)
- [License](#license)
- [Reference](#refecence)

## Introduction

Both `SPSC` and `MPMC` channel are implemented based on pseudocode of [Dmitry Vyukov's queue](https://docs.google.com/document/d/1yIAYmbvL3JxOKOjuCyon7JhW4cSv1wy5hC0ApeGMV9s/pub).
The implementation way is exactly the same. But there are still some differences between them about wait-retry and blocking.<br />

`MPMC` is high contention multithreading environment. If the retry is continuous and immediate, the CPU cache coherence
will be increased rapidly and decrease performance. Therefore, we must wait then retry.
However, this thing is unsuitable in `SPSC` is lower contention multithreading environment (Just 2 threads).
In `SPSC`, the immediate retry still guarantees performance.<br />

The blocking management and optimization in `MPMC` is more complex than `SPSC` 
in spite of the same implementation idea.<br />

Both `SPSC` and `MPMC` channel can be used as queues.<br />

## Usage

Add this to your `Cargo.toml`:
```toml
[dependencies]
omango = "0.1.2"
```

## Compatibility

The minimum supported Rust version is 1.49.

## Benchmarks

Tests were performed on an Intel Core I5 with 4 cores running Windows 10 and
M1 with 8 cores running MacOS BigSur 11.3.

# <img src="./misc/SPSC.png" alt="Omango benchmarks SPSC" width="100%"/>
# <img src="./misc/MPSC.png" alt="Omango benchmarks MPSC" width="100%"/>
# <img src="./misc/MPMC.png" alt="Omango benchmarks MPMC" width="100%"/>

## License

The crate is licensed under the terms of the MIT
license. See [LICENSE](LICENSE) for more information.

#### Third party software

This product includes copies and modifications of software developed by third parties:

* [src/backoff.rs](src/backoff.rs) includes copies and modifications of code from Crossbeam-Utils,
  licensed under the MIT License and the Apache License, Version 2.0.

* [src/cache_padded.rs](src/cache_padded.rs) includes copies and modifications of code from Crossbeam-Utils,
  licensed under the MIT License and the Apache License, Version 2.0.

See the source code files for more details.

The third party licenses can be found in [here](https://github.com/crossbeam-rs/crossbeam/tree/master/crossbeam-utils#LICENSE).

## Reference

* [Dmitry Vyukov's queue](https://docs.google.com/document/d/1yIAYmbvL3JxOKOjuCyon7JhW4cSv1wy5hC0ApeGMV9s/pub)
* [Crossbeam-Channel](https://github.com/crossbeam-rs/crossbeam/tree/master/crossbeam-channel)
* [CppCon 2017: C++ atomics, from basic to advanced](https://www.youtube.com/watch?v=ZQFzMfHIxng)
* [The cache coherence protocols](https://www.sciencedirect.com/topics/engineering/cache-coherence)
