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

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use crc32fast::hash;
use farmhash;

use super::{BlockMeta, SsTable};
use crate::table::{Bloom, FileObject};
use crate::{block::BlockBuilder, key::KeySlice, lsm_storage::BlockCache};
use bytes::BufMut;

/// Builds an SSTable from key-value pairs.
pub struct SsTableBuilder {
    builder: BlockBuilder,
    first_key: Vec<u8>,
    last_key: Vec<u8>,
    data: Vec<u8>,
    key_hashes: Vec<u32>,
    pub(crate) meta: Vec<BlockMeta>,
    block_size: usize,
}

impl SsTableBuilder {
    /// Create a builder based on target block size.
    pub fn new(block_size: usize) -> Self {
        SsTableBuilder {
            builder: BlockBuilder::new(block_size),
            first_key: Vec::new(),
            last_key: Vec::new(),
            data: Vec::new(),
            key_hashes: Vec::new(),
            meta: Vec::new(),
            block_size,
        }
    }

    /// A helper function to split a new block when the current block is full
    /// or when build() is called
    fn split(&mut self) {
        let old_builder = std::mem::replace(&mut self.builder, BlockBuilder::new(self.block_size));
        let block = old_builder.build();
        let encoded_block = block.encode();

        let checksum = hash(&encoded_block);

        let block_meta = BlockMeta {
            offset: self.data.len(),
            first_key: KeySlice::from_slice(&self.first_key)
                .to_key_vec()
                .into_key_bytes(),
            last_key: KeySlice::from_slice(&self.last_key)
                .to_key_vec()
                .into_key_bytes(),
        };

        self.meta.push(block_meta);

        self.data.extend_from_slice(&encoded_block);
        self.data.put_u32(checksum);
    }

    /// Adds a key-value pair to SSTable.
    ///
    /// Note: You should split a new block when the current block is full.(`std::mem::replace` may
    /// be helpful here)
    pub fn add(&mut self, key: KeySlice, value: &[u8]) {
        if self.first_key.is_empty() {
            self.first_key = key.to_key_vec().into_inner();
        }

        if !self.builder.add(key, value) {
            self.split();
            let _ = self.builder.add(key, value);
            self.first_key = key.to_key_vec().into_inner();
        }

        self.last_key = key.to_key_vec().into_inner();

        self.key_hashes.push(farmhash::fingerprint32(key.raw_ref()));
    }

    /// Get the estimated size of the SSTable.
    ///
    /// Since the data blocks contain much more data than meta blocks, just return the size of data
    /// blocks here.
    pub fn estimated_size(&self) -> usize {
        self.data.len()
    }

    /// Builds the SSTable and writes it to the given path. Use the `FileObject` structure to manipulate the disk objects.
    pub fn build(
        #[allow(unused_mut)] mut self,
        id: usize,
        block_cache: Option<Arc<BlockCache>>,
        path: impl AsRef<Path>,
    ) -> Result<SsTable> {
        if !self.builder.is_empty() {
            self.split();
        }

        let block_meta_offset = self.data.len();
        let first_key = self.meta[0].first_key.clone();

        let mut data = self.data;

        BlockMeta::encode_block_meta(&self.meta, &mut data);

        data.put_u32(block_meta_offset as u32);

        let bloom_filter_offset = data.len();

        let bloom = Bloom::build_from_key_hashes(&self.key_hashes, 10);

        bloom.encode(&mut data);

        data.put_u32(bloom_filter_offset as u32);

        let file = FileObject::create(path.as_ref(), data)?;

        Ok(SsTable {
            file,
            id,
            block_meta: self.meta,
            block_meta_offset,
            bloom_filter_offset,
            block_cache,
            first_key: first_key,
            last_key: KeySlice::from_slice(&self.last_key)
                .to_key_vec()
                .into_key_bytes(),
            bloom: Some(bloom),
            max_ts: 0,
        })
    }

    #[cfg(test)]
    pub(crate) fn build_for_test(self, path: impl AsRef<Path>) -> Result<SsTable> {
        self.build(0, None, path)
    }
}
