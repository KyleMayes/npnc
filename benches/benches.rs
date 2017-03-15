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

#[macro_use]
extern crate queuecheck;
extern crate npnc;

use std::env;

const WARMUP: usize = 1_000_000;
const MEASUREMENT: usize = 100_000_000;

fn thousands(ops: f64) -> String {
    let mut string = format!("{:.2}", ops);
    let mut index = string.find('.').unwrap();
    while index > 3 {
        index -= 3;
        string.insert(index, '_');
    }
    string
}

macro_rules! bench {
    ([$($path:tt)*], $producers:expr, $consumers:expr) => ({
        queuecheck_bench_throughput!(
            (WARMUP, MEASUREMENT),
            $producers,
            $consumers,
            |p: &npnc::$($path)*::Producer<i32>, i: i32| p.produce(i).unwrap(),
            |c: &npnc::$($path)*::Consumer<i32>| c.consume().ok()
        )
    });
}

macro_rules! bench_spsc {
    ([$($path:tt)*]$(, $size:expr)*) => ({
        let (producer, consumer) = npnc::$($path)*::channel($($size)*);
        bench!([$($path)*], vec![producer], vec![consumer])
    });
}

macro_rules! run {
    ($filter:expr, $name:expr, $runs:expr, $bench:expr) => ({
        if $filter.as_ref().map_or(true, |f| $name.contains(f)) {
            println!("bench {} ...", $name);
            let mut runs = (0..$runs).map(|_| $bench).collect::<Vec<_>>();
            runs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            println!("  {} operation/second", thousands(runs[$runs / 2]));
        }
    });
}

fn main() {
    let filter = env::args().nth(1).and_then(|f| if f == "--bench" { None } else { Some(f) });
    run!(filter, "bounded_spsc", 25, bench_spsc!([bounded::spsc], 2 << 24));
    run!(filter, "unbounded_spsc", 5, bench_spsc!([unbounded::spsc]));
}
