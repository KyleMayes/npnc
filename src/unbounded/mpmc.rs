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

//! Unbounded multi-producer, multi-consumer lock-free queue.

use std::ptr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use hazard::{Hazard, Memory, VecMemory};

use {ConsumeError, ProduceError, POINTERS};

//================================================
// Structs
//================================================

// Consumer ______________________________________

/// A consumer for an unbounded MPMC lock-free queue.
#[derive(Debug)]
pub struct Consumer<T>(usize, Arc<Queue<T>>);

impl<T> Consumer<T> {
    //- Accessors --------------------------------

    /// Attempts to remove and return the item at the front of the queue.
    ///
    /// This method returns `Err` if the queue is empty.
    pub fn consume(&self) -> Result<T, ConsumeError> {
        self.1.consume(self.0)
    }

    /// Attempts to clone this consumer.
    pub fn try_clone(&self) -> Option<Self> {
        if let Some(thread) = self.1.threads.lock().unwrap().pop() {
            Some(Consumer(thread, self.1.clone()))
        } else {
            None
        }
    }
}

impl<T> Clone for Consumer<T> {
    fn clone(&self) -> Self {
        self.try_clone().expect("too many producer and consumer clones")
    }
}

impl<T> Drop for Consumer<T> {
    fn drop(&mut self) {
        self.1.consumer.store(0, Ordering::Release);
        self.1.threads.lock().unwrap().push(self.0);
    }
}

unsafe impl<T> Send for Consumer<T> where T: Send { }

// Producer __________________________________

/// A producer for an unbounded MPMC lock-free queue.
#[derive(Debug)]
pub struct Producer<T>(usize, Arc<Queue<T>>);

impl<T> Producer<T> {
    //- Accessors --------------------------------

    /// Attempts to add the supplied item to the back of the queue.
    ///
    /// This method returns `Err` if the queue is full or has no remaining consumers.
    pub fn produce(&self, item: T) -> Result<(), ProduceError<T>> {
        self.1.produce(self.0, item)
    }

    /// Attempts to clone this producer.
    pub fn try_clone(&self) -> Option<Self> {
        if let Some(thread) = self.1.threads.lock().unwrap().pop() {
            Some(Producer(thread, self.1.clone()))
        } else {
            None
        }
    }
}

impl<T> Clone for Producer<T> {
    fn clone(&self) -> Self {
        self.try_clone().expect("too many producer and consumer clones")
    }
}

impl<T> Drop for Producer<T> {
    fn drop(&mut self) {
        self.1.producer.store(0, Ordering::Release);
        self.1.threads.lock().unwrap().push(self.0);
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

const READ: usize = 1;
const WRITE: usize = 0;
const NEXT: usize = 2;

#[derive(Debug)]
#[repr(C)]
struct Queue<T> {
    write: AtomicPtr<Node<T>>,
    consumer: AtomicUsize,
    _wpadding: [usize; POINTERS - 2],
    read: AtomicPtr<Node<T>>,
    producer: AtomicUsize,
    _rpadding: [usize; POINTERS - 2],
    hazard: Hazard<Node<T>, VecMemory>,
    threads: Mutex<Vec<usize>>,
}

impl<T> Queue<T> {
    //- Constructors -----------------------------

    fn new(threads: usize) -> Arc<Self> {
        let sentinel = unsafe { VecMemory.allocate(Node::new(None)) };
        Arc::new(Queue {
            write: AtomicPtr::new(sentinel),
            consumer: AtomicUsize::new(1),
            _wpadding: [0; POINTERS - 2],
            read: AtomicPtr::new(sentinel),
            producer: AtomicUsize::new(1),
            _rpadding: [0; POINTERS - 2],
            hazard: Hazard::new(VecMemory, threads, 3),
            threads: Mutex::new((2..threads).collect()),
        })
    }

    //- Accessors --------------------------------

    fn produce(&self, thread: usize, item: T) -> Result<(), ProduceError<T>> {
        // Return an error if all of the consumers have been disconnected.
        if self.consumer.load(Ordering::Acquire) == 0 {
            return Err(ProduceError::Disconnected(item));
        }

        let node = unsafe { VecMemory.allocate(Node::new(Some(item))) };
        loop {
            let write = self.hazard.mark(thread, WRITE, self.write.load(Ordering::SeqCst));
            if write == self.write.load(Ordering::SeqCst) {
                let next = deref!(write).next.load(Ordering::SeqCst);
                if next.is_null() {
                    // Add the item to the back of the queue if this node is available.
                    if exchange(&deref!(write).next, ptr::null_mut(), node) {
                        exchange(&self.write, write, node);
                        self.hazard.clear(thread, WRITE);
                        return Ok(());
                    }
                } else {
                    // Attempt to update the write pointer.
                    exchange(&self.write, write, next);
                }
            }
        }
    }

    fn consume(&self, thread: usize) -> Result<T, ConsumeError> {
        loop {
            // Return an error if the queue is empty.
            let read = self.hazard.mark(thread, READ, self.read.load(Ordering::SeqCst));
            if read == self.write.load(Ordering::SeqCst) {
                if self.producer.load(Ordering::Acquire) == 0 {
                    return Err(ConsumeError::Disconnected);
                } else {
                    return Err(ConsumeError::Empty);
                }
            }

            // Remove and return the item at the front of the queue if this node is available.
            let next = self.hazard.mark(thread, NEXT, deref!(read).next.load(Ordering::SeqCst));
            if exchange(&self.read, read, next) {
                let item = deref_mut!(next).item.take().unwrap();
                self.hazard.clear(thread, READ);
                self.hazard.clear(thread, NEXT);
                self.hazard.retire(thread, read);
                return Ok(item);
            }
        }
    }
}

impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        while self.consume(0).is_ok() { }
        unsafe { VecMemory.deallocate(self.write.load(Ordering::Relaxed)); }
    }
}

unsafe impl<T> Sync for Queue<T> where T: Send { }

//================================================
// Functions
//================================================

fn exchange<T>(atomic: &AtomicPtr<Node<T>>, current: *mut Node<T>, new: *mut Node<T>) -> bool {
    atomic.compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst).is_ok()
}

/// Returns a producer and consumer for an unbounded MPMC lock-free queue.
///
/// The value of `clones` indicates the maximum number of clones allowed of the initial producer
/// and consumer. Both types of clones count towards this total.
pub fn channel<T>(clones: usize) -> (Producer<T>, Consumer<T>) {
    let queue = Queue::new(clones + 2);
    (Producer(0, queue.clone()), Consumer(1, queue))
}
