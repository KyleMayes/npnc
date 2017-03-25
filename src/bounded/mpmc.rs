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

//! Bounded multi-producer, multi-consumer lock-free queue.

use std::mem;
use std::ptr;
use std::cell::{UnsafeCell};
use std::sync::{Arc};
use std::sync::atomic::{AtomicUsize};
use std::sync::atomic::Ordering::*;

use {ConsumeError, ProduceError, POINTERS};
use buffer::{Buffer};

//================================================
// Structs
//================================================

// Consumer ______________________________________

/// A consumer for a bounded MPMC lock-free queue.
#[derive(Debug)]
pub struct Consumer<T>(Arc<Queue<T>>);

impl<T> Consumer<T> {
    //- Accessors --------------------------------

    /// Attempts to remove and return the item at the front of the queue.
    ///
    /// This method returns `Err` if the queue is empty.
    pub fn consume(&self) -> Result<T, ConsumeError> {
        self.0.consume()
    }
}

impl<T> Clone for Consumer<T> {
    fn clone(&self) -> Self {
        self.0.consumer.fetch_add(1, Release);
        Consumer(self.0.clone())
    }
}

impl<T> Drop for Consumer<T> {
    fn drop(&mut self) {
        self.0.consumer.fetch_sub(1, Release);
    }
}

unsafe impl<T> Send for Consumer<T> where T: Send { }

// Producer __________________________________

/// A producer for a bounded MPMC lock-free queue.
#[derive(Debug)]
pub struct Producer<T>(Arc<Queue<T>>);

impl<T> Producer<T> {
    //- Accessors --------------------------------

    /// Attempts to add the supplied item to the back of the queue.
    ///
    /// This method returns `Err` if the queue is full or has no remaining consumers.
    pub fn produce(&self, item: T) -> Result<(), ProduceError<T>> {
        self.0.produce(item)
    }
}

impl<T> Clone for Producer<T> {
    fn clone(&self) -> Self {
        self.0.producer.fetch_add(1, Release);
        Producer(self.0.clone())
    }
}

impl<T> Drop for Producer<T> {
    fn drop(&mut self) {
        self.0.producer.fetch_sub(1, Release);
    }
}

unsafe impl<T> Send for Producer<T> where T: Send { }

// Slot __________________________________________

#[derive(Debug)]
struct Slot<T> {
    item: UnsafeCell<T>,
    sequence: AtomicUsize,
}

impl<T> Slot<T> {
    //- Constructors -----------------------------

    fn new(index: usize) -> Self {
        let item = unsafe { mem::uninitialized() };
        Slot { item: UnsafeCell::new(item), sequence: AtomicUsize::new(index) }
    }

    //- Accessors --------------------------------

    unsafe fn get(&self) -> T {
        let mut item = mem::uninitialized();
        ptr::swap(self.item.get(), &mut item);
        item
    }

    unsafe fn set(&self, item: T) {
        ptr::write(self.item.get(), item);
    }
}

// Queue _________________________________________

#[derive(Debug)]
#[repr(C)]
struct Queue<T> {
    write: AtomicUsize,
    consumer: AtomicUsize,
    _wpadding: [usize; POINTERS - 2],
    read: AtomicUsize,
    producer: AtomicUsize,
    _rpadding: [usize; POINTERS - 2],
    buffer: Buffer<Slot<T>>,
}

impl<T> Queue<T> {
    //- Constructors -----------------------------

    fn new(size: usize) -> Arc<Self> {
        let buffer = Buffer::new(size);
        for index in 0..size {
            unsafe { buffer.set(index, Slot::new(index)); }
        }
        Arc::new(Queue {
            write: AtomicUsize::new(0),
            consumer: AtomicUsize::new(1),
            _wpadding: [0; POINTERS - 2],
            read: AtomicUsize::new(0),
            producer: AtomicUsize::new(1),
            _rpadding: [0; POINTERS - 2],
            buffer: buffer,
        })
    }

    //- Accessors --------------------------------

    fn produce(&self, item: T) -> Result<(), ProduceError<T>> {
        // Return an error if all of the consumers have been disconnected.
        if self.consumer.load(Acquire) == 0 {
            return Err(ProduceError::Disconnected(item));
        }

        loop {
            let write = self.write.load(Relaxed);
            let slot = unsafe { self.buffer.wrapping_get_ref(write) };
            let sequence = slot.sequence.load(Acquire);
            let difference = (sequence as isize).wrapping_sub(write as isize);

            // Return an error if the queue is full.
            if difference < 0 {
                return Err(ProduceError::Full(item));
            }

            // Add the item to the back of the queue if this slot is available.
            let next = write.wrapping_add(1);
            if difference == 0 && exchange(&self.write, write, next) {
                unsafe { slot.set(item); }
                slot.sequence.store(next, Release);
                return Ok(());
            }
        }
    }

    fn consume(&self) -> Result<T, ConsumeError> {
        loop {
            let read = self.read.load(Relaxed);
            let slot = unsafe { self.buffer.wrapping_get_ref(read) };
            let sequence = slot.sequence.load(Acquire);
            let difference = (sequence as isize).wrapping_sub(read.wrapping_add(1) as isize);

            // Return an error if the queue is empty.
            if difference < 0 {
                if self.producer.load(Acquire) == 0 {
                    return Err(ConsumeError::Disconnected);
                } else {
                    return Err(ConsumeError::Empty);
                }
            }

            // Remove and return the item at the front of the queue if this slot is available.
            let next = read.wrapping_add(1);
            if difference == 0 && exchange(&self.read, read, next) {
                let item = unsafe { slot.get() };
                slot.sequence.store(next.wrapping_add(self.buffer.size() - 1), Release);
                return Ok(item);
            }
        }
    }
}

impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        while self.consume().is_ok() { }
    }
}

unsafe impl<T> Sync for Queue<T> where T: Send { }

//================================================
// Functions
//================================================

fn exchange(atomic: &AtomicUsize, current: usize, new: usize) -> bool {
    atomic.compare_exchange_weak(current, new, Relaxed, Relaxed).is_ok()
}

/// Returns a producer and consumer for a bounded MPMC lock-free queue.
///
/// # Panics
///
/// * `size` is not a power of two
pub fn channel<T>(size: usize) -> (Producer<T>, Consumer<T>) {
    assert!(size.is_power_of_two(), "`size` is not a power of two");
    let queue = Queue::new(size);
    (Producer(queue.clone()), Consumer(queue))
}
