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

mod builder;
mod iterator;

pub use builder::BlockBuilder;
use bytes::{Buf, BufMut, Bytes, BytesMut};
pub use iterator::BlockIterator;

/// A block is the smallest unit of read and caching in LSM tree. It is a collection of sorted key-value pairs.
pub struct Block {
    pub(crate) data: Vec<u8>,
    pub(crate) offsets: Vec<u16>,
}

impl Block {
    /// Encode the internal data to the data layout illustrated in the course
    /// Note: You may want to recheck if any of the expected field is missing from your output
    pub fn encode(&self) -> Bytes {
        let total_size = self.data.len() + self.offsets.len() * 2 + 2;
        let mut buf = BytesMut::with_capacity(total_size);

        buf.put_slice(&self.data);

        for &offset in &self.offsets {
            buf.put_u16(offset);
        }

        buf.put_u16(self.offsets.len() as u16);

        buf.freeze()
    }

    /// Decode from the data layout, transform the input `data` to a single `Block`
    pub fn decode(data: &[u8]) -> Self {
        let num_of_elements = (&data[data.len() - 2..]).get_u16() as usize;

        let offsets_start = data.len() - 2 - num_of_elements * 2;

        let raw_data = data[..offsets_start].to_vec();

        let mut offsets = Vec::with_capacity(num_of_elements);
        let mut offsets_ptr = &data[offsets_start..data.len() - 2];
        for _ in 0..num_of_elements {
            offsets.push(offsets_ptr.get_u16());
        }

        Block {
            data: raw_data,
            offsets,
        }
    }
}
