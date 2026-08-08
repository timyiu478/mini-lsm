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

use std::sync::Arc;

use anyhow::Result;

use super::SsTable;
use crate::{block::BlockIterator, iterators::StorageIterator, key::KeySlice};
use crate::block::Block;

/// An iterator over the contents of an SSTable.
pub struct SsTableIterator {
    table: Arc<SsTable>,
    blk_iter: BlockIterator,
    blk_idx: usize,
}

impl SsTableIterator {
    /// Create a new iterator and seek to the first key-value pair in the first data block.
    pub fn create_and_seek_to_first(table: Arc<SsTable>) -> Result<Self> {
        let block = table.read_block(0)?;

        Ok(SsTableIterator {
            table,
            blk_iter: BlockIterator::create_and_seek_to_first(block),
            blk_idx: 0,
        })
    }

    /// Seek to the first key-value pair in the first data block.
    pub fn seek_to_first(&mut self) -> Result<()> {
        let block = self.table.read_block(0)?;
        
        self.blk_iter = BlockIterator::create_and_seek_to_first(block);
        self.blk_idx = 0;

        Ok(())
    }

    /// Create a new iterator and seek to the first key-value pair which >= `key`.
    pub fn create_and_seek_to_key(table: Arc<SsTable>, key: KeySlice) -> Result<Self> {
        let mut blk_idx = table.find_block_idx(key);
        let block = table.read_block(blk_idx)?;
        let mut blk_iter = BlockIterator::create_and_seek_to_key(block, key);

        // If the search key is greater than all keys in the current block, 
        // the iterator will become invalid. We must fall over to the next block.
        if !blk_iter.is_valid() && blk_idx + 1 < table.block_meta.len() {
            blk_idx += 1;
            let next_block = table.read_block(blk_idx)?;
            // Since we know the next block's keys are all greater than our search key,
            // we just seek to its very first key.
            blk_iter = BlockIterator::create_and_seek_to_first(next_block);
        }

        Ok(SsTableIterator {
            table,
            blk_iter,
            blk_idx,
        })
    }

    /// Seek to the first key-value pair which >= `key`.
    /// Note: You probably want to review the handout for detailed explanation when implementing
    /// this function.
    pub fn seek_to_key(&mut self, key: KeySlice) -> Result<()> {
        let mut blk_idx = self.table.find_block_idx(key);
        let block = self.table.read_block(blk_idx)?;
        let mut blk_iter = BlockIterator::create_and_seek_to_key(block, key);

        if !blk_iter.is_valid() && blk_idx + 1 < self.table.block_meta.len() {
            blk_idx += 1;
            let next_block = self.table.read_block(blk_idx)?;
            blk_iter = BlockIterator::create_and_seek_to_first(next_block);
        }

        self.blk_iter = blk_iter;
        self.blk_idx = blk_idx;

        Ok(())
    }
}

impl StorageIterator for SsTableIterator {
    type KeyType<'a> = KeySlice<'a>;

    /// Return the `key` that's held by the underlying block iterator.
    fn key(&self) -> KeySlice<'_> {
        self.blk_iter.key()
    }

    /// Return the `value` that's held by the underlying block iterator.
    fn value(&self) -> &[u8] {
        self.blk_iter.value()
    }

    /// Return whether the current block iterator is valid or not.
    fn is_valid(&self) -> bool {
        self.blk_iter.is_valid()
    }

    /// Move to the next `key` in the block.
    /// Note: You may want to check if the current block iterator is valid after the move.
    fn next(&mut self) -> Result<()> {
        self.blk_iter.next();

        // If the current block iterator is exhausted and there are more blocks
        if !self.blk_iter.is_valid() && self.blk_idx + 1 < self.table.block_meta.len() {
            self.blk_idx += 1;
            
            let block = self.table.read_block(self.blk_idx)?;
            self.blk_iter = BlockIterator::create_and_seek_to_first(block);
        }

        Ok(())
    }
}
