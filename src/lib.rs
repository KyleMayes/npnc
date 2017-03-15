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

//! Lock-free queues.

#![cfg_attr(feature="valgrind", feature(alloc_system))]

#![warn(missing_copy_implementations, missing_debug_implementations, missing_docs)]

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]
#![cfg_attr(feature="clippy", warn(clippy))]

#[cfg(feature="valgrind")]
extern crate alloc_system;

extern crate hazard;

use std::error;
use std::fmt;

#[macro_use]
mod utility;
mod buffer;
pub mod bounded;
pub mod unbounded;

/// The number of pointers that fit in a 128 byte cacheline.
#[cfg(target_pointer_width="32")]
const POINTERS: usize = 32;
#[cfg(target_pointer_width="64")]
const POINTERS: usize = 16;

//================================================
// Enums
//================================================

// ConsumeError __________________________________

/// Indicates the reason a `consume` operation could not return an item.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ConsumeError {
    /// The queue was empty and had no remaining producers.
    Disconnected,
    /// The queue was empty.
    Empty,
}

impl error::Error for ConsumeError {
    fn description(&self) -> &str {
        match *self {
            ConsumeError::Disconnected => "the queue was empty and had no remaining producers",
            ConsumeError::Empty => "the queue was empty",
        }
    }
}

impl fmt::Display for ConsumeError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{}", error::Error::description(self))
    }
}

// ProduceError __________________________________

/// Indicates the reason a `produce` operation rejected an item.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ProduceError<T> {
    /// The queue had no remaining consumers.
    Disconnected(T),
    /// The queue was full.
    Full(T),
}

impl<T> ProduceError<T> {
    //- Consumers --------------------------------

    /// Returns the rejected item.
    pub fn item(self) -> T {
        match self { ProduceError::Disconnected(item) | ProduceError::Full(item) => item }
    }
}

impl<T> error::Error for ProduceError<T> {
    fn description(&self) -> &str {
        match *self {
            ProduceError::Disconnected(_) => "the queue had no remaining consumers",
            ProduceError::Full(_) => "the queue was full",
        }
    }
}

impl<T> fmt::Debug for ProduceError<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ProduceError::Disconnected(_) => write!(formatter, "ProduceError::Disconnected(..)"),
            ProduceError::Full(_) => write!(formatter, "ProduceError::Full(..)"),
        }
    }
}

impl<T> fmt::Display for ProduceError<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{}", error::Error::description(self))
    }
}
