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

use std::mem;
use std::ptr;

//================================================
// Structs
//================================================

// Buffer ________________________________________

/// A fixed size buffer.
#[derive(Debug)]
pub struct Buffer<T> {
    data: *mut T,
    size: usize,
}

impl<T> Buffer<T> {
    //- Constructors -----------------------------

    /// Constructs a new `Buffer`,
    pub fn new(size: usize) -> Self {
        assert!(size.is_power_of_two());
        let mut vec = Vec::with_capacity(size);
        let data = vec.as_mut_ptr();
        mem::forget(vec);
        Buffer { data: data, size: size }
    }

    //- Accessors --------------------------------

    /// Returns the size of this buffer.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns the item at the supplied index in this buffer.
    pub unsafe fn get(&self, index: usize) -> T {
        let mut item = mem::uninitialized();
        ptr::swap(self.data.offset(index as isize), &mut item);
        item
    }

    /// Returns the item at the supplied index in this buffer after wrapping the index.
    pub unsafe fn wrapping_get(&self, index: usize) -> T {
        self.get(index & (self.size - 1))
    }

    /// Sets the item at the supplied index in this buffer.
    pub unsafe fn set(&self, index: usize, item: T) {
        ptr::write(self.data.offset(index as isize), item);
    }

    /// Returns the item at the supplied index in this buffer after wrapping the index.
    pub unsafe fn wrapping_set(&self, index: usize, item: T) {
        self.set(index & (self.size - 1), item);
    }
}

impl<T> Drop for Buffer<T> {
    fn drop(&mut self) {
        unsafe { Vec::from_raw_parts(self.data, 0, self.size); }
    }
}
