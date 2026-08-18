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
        // 1. Ensure buffer has at least 2 bytes for num_of_elements
        assert!(data.len() >= 2, "data slice too small");

        let num_of_elements = (&data[data.len() - 2..]).get_u16() as usize;

        // 2. Ensure buffer is large enough for offsets array + num_of_elements u16
        assert!(
            data.len() >= 2 + num_of_elements * 2,
            "data slice smaller than offset table"
        );

        let offsets_start = data.len() - 2 - num_of_elements * 2;
        let raw_data = data[..offsets_start].to_vec();

        let mut offsets = Vec::with_capacity(num_of_elements);
        let mut offsets_ptr = &data[offsets_start..data.len() - 2];

        for i in 0..num_of_elements {
            let offset = offsets_ptr.get_u16();

            // 3. Compare current offset with previous offset (offsets[i - 1])
            if i > 0 {
                assert!(
                    offsets[i - 1] < offset,
                    "offsets are not strictly increasing"
                );
            }

            offsets.push(offset);
        }

        // 4. Safely validate the last offset bound if elements exist
        if let Some(&last_offset) = offsets.last() {
            let last_offset = last_offset as usize;

            assert!(
                last_offset + 14 <= offsets_start,
                "last entry header exceeds data section"
            );

            let rest_key_len = (&raw_data[last_offset + 2..last_offset + 4]).get_u16() as usize;

            let val_len_offset = last_offset + 4 + rest_key_len + 8;

            assert!(
                val_len_offset + 2 <= offsets_start,
                "key extends past data section"
            );

            let val_len = (&raw_data[val_len_offset..val_len_offset + 2]).get_u16() as usize;
            let entry_end = val_len_offset + 2 + val_len;
            assert!(
                entry_end <= offsets_start,
                "value extends past data section"
            );
        }

        Block {
            data: raw_data,
            offsets,
        }
    }
}
