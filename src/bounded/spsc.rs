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

//! Bounded single-producer, single-consumer wait-free queue.

use std::cell::{Cell};
use std::sync::{Arc};
use std::sync::atomic::{AtomicUsize};
use std::sync::atomic::Ordering::*;

use {ConsumeError, ProduceError, POINTERS};
use buffer::{Buffer};

//================================================
// Structs
//================================================

// Consumer ______________________________________

/// A consumer for a bounded SPSC wait-free queue.
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

    /// Returns the number of items currently in the queue.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the queue is currently empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the maximum number of items the queue can contain.
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }
}

impl<T> Drop for Consumer<T> {
    fn drop(&mut self) {
        self.0.consumer.store(0, Release);
    }
}

unsafe impl<T> Send for Consumer<T> where T: Send { }

// Producer __________________________________

/// A producer for a bounded SPSC wait-free queue.
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

    /// Returns the number of items currently in the queue.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the queue is currently empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the maximum number of items the queue can contain.
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }
}

impl<T> Drop for Producer<T> {
    fn drop(&mut self) {
        self.0.producer.store(0, Release);
    }
}

unsafe impl<T> Send for Producer<T> where T: Send { }

// Queue _________________________________________

#[derive(Debug)]
#[repr(C)]
struct Queue<T> {
    write: AtomicUsize,
    read_copy: Cell<usize>,
    consumer: AtomicUsize,
    _wpadding: [usize; POINTERS - 3],
    read: AtomicUsize,
    write_copy: Cell<usize>,
    producer: AtomicUsize,
    _rpadding: [usize; POINTERS - 3],
    buffer: Buffer<T>,
}

impl<T> Queue<T> {
    //- Constructors -----------------------------

    fn new(size: usize) -> Arc<Self> {
        Arc::new(Queue {
            write: AtomicUsize::new(0),
            read_copy: Cell::new(0),
            consumer: AtomicUsize::new(1),
            _wpadding: [0; POINTERS - 3],
            read: AtomicUsize::new(0),
            write_copy: Cell::new(0),
            producer: AtomicUsize::new(1),
            _rpadding: [0; POINTERS - 3],
            buffer: Buffer::new(size),
        })
    }

    //- Accessors --------------------------------

    fn len(&self) -> usize {
        self.write.load(Acquire).wrapping_sub(self.read.load(Acquire))
    }

    fn capacity(&self) -> usize {
        self.buffer.size()
    }

    fn produce(&self, item: T) -> Result<(), ProduceError<T>> {
        // Return an error if the consumer has been disconnected.
        if self.consumer.load(Acquire) == 0 {
            return Err(ProduceError::Disconnected(item));
        }

        // Return an error if the queue is full.
        let write = self.write.load(Acquire);
        if write.wrapping_sub(self.read_copy.get()) == self.buffer.size() {
            self.read_copy.set(self.read.load(Acquire));
            if write.wrapping_sub(self.read_copy.get()) == self.buffer.size() {
                return Err(ProduceError::Full(item));
            }
        }

        // Add the item to the back of the queue.
        unsafe { self.buffer.wrapping_set(write, item); }
        self.write.store(write.wrapping_add(1), Release);
        Ok(())
    }

    fn consume(&self) -> Result<T, ConsumeError> {
        // Return an error if the queue is empty.
        let read = self.read.load(Acquire);
        if read == self.write_copy.get() {
            self.write_copy.set(self.write.load(Acquire));
            if read == self.write_copy.get() {
                if self.producer.load(Acquire) == 0 {
                    return Err(ConsumeError::Disconnected);
                } else {
                    return Err(ConsumeError::Empty);
                }
            }
        }

        // Remove and return the item at the front of the queue.
        let item = unsafe { self.buffer.wrapping_get(read) };
        self.read.store(read.wrapping_add(1), Release);
        Ok(item)
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

/// Returns a producer and consumer for a bounded SPSC wait-free queue.
///
/// # Panics
///
/// * `size` is not a power of two
pub fn channel<T>(size: usize) -> (Producer<T>, Consumer<T>) {
    assert!(size.is_power_of_two(), "`size` is not a power of two");
    let queue = Queue::new(size);
    (Producer(queue.clone()), Consumer(queue))
}
