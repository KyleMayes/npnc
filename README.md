# npnc

[![crates.io](https://img.shields.io/crates/v/npnc.svg)](https://crates.io/crates/npnc)
[![docs.rs](https://docs.rs/npnc/badge.svg)](https://docs.rs/npnc)
[![Travis CI](https://travis-ci.org/KyleMayes/npnc.svg?branch=master)](https://travis-ci.org/KyleMayes/npnc)

Lock-free queues.

Supported on the stable, beta, and nightly Rust channels.

Released under the Apache License 2.0.

## Features

 * Bounded lock-free SPSC queue
 * Bounded lock-free MPMC queue
 * Unbounded lock-free SPSC queue
 * Unbounded lock-free MPMC queue

## Examples

### Bounded SPSC

```rust
extern crate npnc;

use std::thread;

use npnc::bounded::spsc;

fn main() {
    let (producer, consumer) = spsc::channel(64);

    // Producer
    let b = thread::spawn(move || {
        for index in 0..32 {
            producer.produce(index).unwrap();
        }
    });

    // Consumer
    let a = thread::spawn(move || {
        loop {
            if let Ok(item) = consumer.consume() {
                println!("{}", item);
                if item == 31 {
                    break;
                }
            }
        }
    });

    a.join().unwrap();
    b.join().unwrap();
}
```
