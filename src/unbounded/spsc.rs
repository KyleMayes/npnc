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

//! Unbounded single-producer, single-consumer wait-free queue.

use std::ptr;
use std::cell::{Cell};
use std::sync::{Arc};
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use hazard::{Memory, VecMemory};

use {ConsumeError, ProduceError, POINTERS};

//================================================
// Structs
//================================================

// Consumer ______________________________________

/// A consumer for an unbounded SPSC wait-free queue.
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

impl<T> Drop for Consumer<T> {
    fn drop(&mut self) {
        self.0.consumer.store(0, Ordering::Release);
    }
}

unsafe impl<T> Send for Consumer<T> where T: Send { }

// Producer __________________________________

/// A producer for an unbounded SPSC wait-free queue.
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

impl<T> Drop for Producer<T> {
    fn drop(&mut self) {
        self.0.producer.store(0, Ordering::Release);
    }
}

unsafe impl<T> Send for Producer<T> where T: Send { }

// Node __________________________________________

#[derive(Debug)]
struct Node<T> {
    item: Option<T>,
    next: AtomicPtr<Node<T>>,
}

impl<T> Node<T> {
    //- Constructors -----------------------------

    fn new(item: Option<T>) -> Self {
        Node { item: item, next: AtomicPtr::new(ptr::null_mut()) }
    }
}

// Queue _________________________________________

#[derive(Debug)]
#[repr(C)]
struct Queue<T> {
    write: Cell<*mut Node<T>>,
    consumer: AtomicUsize,
    _wpadding: [usize; POINTERS - 2],
    read: Cell<*mut Node<T>>,
    producer: AtomicUsize,
    _rpadding: [usize; POINTERS - 2],
}

impl<T> Queue<T> {
    //- Constructors -----------------------------

    fn new() -> Arc<Self> {
        let sentinel = unsafe { VecMemory.allocate(Node::new(None)) };
        Arc::new(Queue {
            write: Cell::new(sentinel),
            consumer: AtomicUsize::new(1),
            _wpadding: [0; POINTERS - 2],
            read: Cell::new(sentinel),
            producer: AtomicUsize::new(1),
            _rpadding: [0; POINTERS - 2],
        })
    }

    //- Accessors --------------------------------

    fn produce(&self, item: T) -> Result<(), ProduceError<T>> {
        // Return an error if the consumer has been disconnected.
        if self.consumer.load(Ordering::Acquire) == 0 {
            return Err(ProduceError::Disconnected(item));
        }

        // Add the item to the back of the queue.
        let node = unsafe { VecMemory.allocate(Node::new(Some(item))) };
        deref!(self.write.get()).next.store(node, Ordering::Release);
        self.write.set(node);
        Ok(())
    }

    fn consume(&self) -> Result<T, ConsumeError> {
        // Return an error if the queue is empty.
        let next = deref!(self.read.get()).next.load(Ordering::Acquire);
        if next.is_null() {
            if self.producer.load(Ordering::Acquire) == 0 {
                return Err(ConsumeError::Disconnected);
            } else {
                return Err(ConsumeError::Empty);
            }
        }

        // Remove and return the item at the front of the queue.
        let item = deref_mut!(next).item.take().unwrap();
        unsafe { VecMemory.deallocate(self.read.get()); }
        self.read.set(next);
        Ok(item)
    }
}

impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        while self.consume().is_ok() { }
        unsafe { VecMemory.deallocate(self.write.get()); }
    }
}

unsafe impl<T> Sync for Queue<T> where T: Send { }

//================================================
// Functions
//================================================

/// Returns a producer and consumer for an unbounded SPSC wait-free queue.
pub fn channel<T>() -> (Producer<T>, Consumer<T>) {
    let queue = Queue::new();
    (Producer(queue.clone()), Consumer(queue))
}
