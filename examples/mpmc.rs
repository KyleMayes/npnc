// Copyright 2018 Kyle Mayes
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
