// Copyright (c) 2022-2026 Alex Chi Z
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

use std::cmp::{self};
use std::collections::BinaryHeap;
use std::collections::binary_heap::PeekMut;

use anyhow::Result;

use crate::key::KeySlice;

use super::StorageIterator;

struct HeapWrapper<I: StorageIterator>(pub usize, pub Box<I>);

impl<I: StorageIterator> PartialEq for HeapWrapper<I> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == cmp::Ordering::Equal
    }
}

impl<I: StorageIterator> Eq for HeapWrapper<I> {}

impl<I: StorageIterator> PartialOrd for HeapWrapper<I> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<I: StorageIterator> Ord for HeapWrapper<I> {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.1
            .key()
            .cmp(&other.1.key())
            .then(self.0.cmp(&other.0))
            .reverse()
    }
}

/// Merge multiple iterators of the same type. If the same key occurs multiple times in some
/// iterators, prefer the one with smaller index.
pub struct MergeIterator<I: StorageIterator> {
    iters: BinaryHeap<HeapWrapper<I>>,
    current: Option<HeapWrapper<I>>,
}

impl<I: StorageIterator> MergeIterator<I> {
    pub fn create(iters: Vec<Box<I>>) -> Self {
        let mut heap = BinaryHeap::new();
        for (idx, iter) in iters.into_iter().enumerate() {
            if iter.is_valid() {
                heap.push(HeapWrapper(idx, iter));
            }
        }

        let current = heap.pop();

        MergeIterator {
            iters: heap,
            current,
        }
    }
}

impl<I: 'static + for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>> StorageIterator
    for MergeIterator<I>
{
    type KeyType<'a> = KeySlice<'a>;

    fn key(&self) -> KeySlice<'_> {
        self.current
            .as_ref()
            .map_or(KeySlice::default(), |wrapper| wrapper.1.key())
    }

    fn value(&self) -> &[u8] {
        self.current
            .as_ref()
            .map_or(&[], |wrapper| wrapper.1.value())
    }

    fn is_valid(&self) -> bool {
        self.current
            .as_ref()
            .map_or(false, |wrapper| wrapper.1.is_valid())
    }

    fn next(&mut self) -> Result<()> {
        let current = match self.current.as_mut() {
            Some(current) => current,
            None => return Ok(()),
        };

        // Loop through `iters` to advance and skip any sibling iterators that hold the exact same key!
        while let Some(mut top) = self.iters.peek_mut() {
            if top.1.key() == current.1.key() {
                if let Err(e) = top.1.next() {
                    PeekMut::pop(top);
                    return Err(e);
                }

                if !top.1.is_valid() {
                    PeekMut::pop(top);
                }
            } else {
                break;
            }
        }

        if let Err(e) = current.1.next() {
            self.current = self.iters.pop();
            return Err(e);
        }

        // Re-evaluate `current`
        if current.1.is_valid() {
            if let Some(old_current) = self.current.take() {
                self.iters.push(old_current);
            }
        }

        self.current = self.iters.pop();

        Ok(())
    }
}
