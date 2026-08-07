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

#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::sync::Arc;

use crate::key::{KeySlice, KeyVec};

use super::Block;

/// Iterates on a block.
pub struct BlockIterator {
    /// The internal `Block`, wrapped by an `Arc`
    block: Arc<Block>,
    /// The current key, empty represents the iterator is invalid
    key: KeyVec,
    /// the current value range in the block.data, corresponds to the current key
    value_range: (usize, usize),
    /// Current index of the key-value pair, should be in range of [0, num_of_elements)
    idx: usize,
    /// The first key in the block
    first_key: KeyVec,
}

impl BlockIterator {
    fn new(block: Arc<Block>) -> Self {
        Self {
            block,
            key: KeyVec::new(),
            value_range: (0, 0),
            idx: 0,
            first_key: KeyVec::new(),
        }
    }

    /// Creates a block iterator and seek to the first entry.
    pub fn create_and_seek_to_first(block: Arc<Block>) -> Self {
        let mut block_iter = BlockIterator::new(block);
        block_iter.seek_to_first();
        block_iter
    }

    /// Creates a block iterator and seek to the first key that >= `key`.
    pub fn create_and_seek_to_key(block: Arc<Block>, key: KeySlice) -> Self {
        let mut block_iter = BlockIterator::new(block);
        block_iter.seek_to_key(key);
        block_iter
    }

    /// Returns the key of the current entry.
    pub fn key(&self) -> KeySlice<'_> {
        self.key.as_key_slice()
    }

    /// Returns the value of the current entry.
    pub fn value(&self) -> &[u8] {
        &self.block.data[self.value_range.0..self.value_range.1]
    }

    /// Returns true if the iterator is valid.
    /// Note: You may want to make use of `key`
    pub fn is_valid(&self) -> bool {
        self.idx < self.block.offsets.len()
    }

    /// Helper method to decode entry at a given element index.
    fn seek_to_index(&mut self, idx: usize) {
        self.idx = idx;

        // If index is out of bounds or block is empty, set iterator to invalid state
        if self.idx >= self.block.offsets.len() {
            self.key.clear();
            self.value_range = (0, 0);
            return;
        }

        let offset = self.block.offsets[self.idx] as usize;

        // 1. Parse Key Length & Key
        let key_len_bytes = self.block.data[offset..offset + 2]
            .try_into()
            .expect("slice with incorrect length");
        let key_len = u16::from_be_bytes(key_len_bytes) as usize;

        let key = &self.block.data[(offset + 2)..(offset + 2 + key_len)];
        self.key = KeySlice::from_slice(key).to_key_vec();

        // 2. Parse Value Length & Value Range
        let val_len_offset = offset + 2 + key_len;
        let val_len_bytes = self.block.data[val_len_offset..val_len_offset + 2]
            .try_into()
            .expect("slice with incorrect length");
        let val_len = u16::from_be_bytes(val_len_bytes) as usize;

        let val_start = val_len_offset + 2;
        self.value_range = (val_start, val_start + val_len);
    }

    /// Seeks to the first key in the block.
    pub fn seek_to_first(&mut self) {
        self.seek_to_index(0);
        if self.is_valid() {
            self.first_key = self.key.clone();
        }
    }

    /// Move to the next key in the block.
    pub fn next(&mut self) {
        self.seek_to_index(self.idx + 1);
    }

    /// Seek to the first key that >= `key`.
    /// Note: You should assume the key-value pairs in the block are sorted when being added by
    /// callers.
    pub fn seek_to_key(&mut self, key: KeySlice) {
        let mut left = 0;
        let mut right = self.block.offsets.len();

        while left < right {
            let mid = (left + right) / 2;
            let offset = self.block.offsets[mid] as usize;

            let key_len_bytes = self.block.data[offset..offset + 2]
                .try_into()
                .expect("slice with incorrect length");
            let key_len = u16::from_be_bytes(key_len_bytes) as usize;

            let key_slice =
                KeySlice::from_slice(&self.block.data[(offset + 2)..(offset + 2 + key_len)]);

            if key_slice < key {
                left = mid + 1;
            } else {
                right = mid;
            }
        }

        self.seek_to_index(left);
    }
}
