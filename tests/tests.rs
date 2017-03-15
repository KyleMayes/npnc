// Copyright 2017 Kyle Mayes
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(feature="valgrind", feature(alloc_system))]

#[cfg(feature="valgrind")]
extern crate alloc_system;

#[macro_use]
extern crate queuecheck;
extern crate npnc;

use std::env;

#[cfg(feature="valgrind")]
const OPERATIONS: usize = 100_000;
#[cfg(not(feature="valgrind"))]
const OPERATIONS: usize = 1_000_000;

macro_rules! test {
    ([$($path:tt)*], $producers:expr, $consumers:expr) => ({
        queuecheck_test!(
            OPERATIONS,
            $producers,
            $consumers,
            |p: &npnc::$($path)*::Producer<String>, i: String| p.produce(i).unwrap(),
            |c: &npnc::$($path)*::Consumer<String>| c.consume().ok()
        );
    });
}

macro_rules! test_mpmc {
    ([$($path:tt)*]$(, $size:expr)*) => ({
        let (producer, consumer) = npnc::$($path)*::channel($($size)*);
        test!([$($path)*], vec![producer.clone(), producer], vec![consumer.clone(), consumer]);
    });
}

macro_rules! test_spsc {
    ([$($path:tt)*]$(, $size:expr)*) => ({
        let (producer, consumer) = npnc::$($path)*::channel($($size)*);
        test!([$($path)*], vec![producer], vec![consumer]);
    });
}

macro_rules! run {
    ($filter:expr, $name:expr, $test:expr) => ({
        if $filter.as_ref().map_or(true, |f| $name.contains(f)) {
            println!("test {} ...", $name);
            $test;
        }
    });
}

fn main() {
    let filter = env::args().nth(1);
    run!(filter, "bounded_spsc", test_spsc!([bounded::spsc], 2 << 24));
    run!(filter, "unbounded_spsc", test_spsc!([unbounded::spsc]));
    run!(filter, "bounded_mpmc", test_mpmc!([bounded::mpmc], 2 << 24));
}
