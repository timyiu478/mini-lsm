// REMOVE THIS LINE after fully implementing this functionality
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

use crate::key::{KeyBytes, KeySlice};
use anyhow::Result;
use anyhow::bail;
use bytes::Bytes;
use bytes::{Buf, BufMut, BytesMut};
use crossbeam_skiplist::SkipMap;
use parking_lot::Mutex;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::sync::Arc;

pub struct Wal {
    file: Arc<Mutex<BufWriter<File>>>,
}

impl Wal {
    pub fn create(_path: impl AsRef<Path>) -> Result<Self> {
        let file = File::options().create(true).append(true).open(_path)?;
        Ok(Wal {
            file: Arc::new(Mutex::new(BufWriter::new(file))),
        })
    }

    pub fn recover(_path: impl AsRef<Path>, _skiplist: &SkipMap<KeyBytes, Bytes>) -> Result<Self> {
        let mut file = File::options().read(true).append(true).open(_path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let mut ptr = buf.as_slice();
        let mut valid_offset = 0;

        while !ptr.is_empty() {
            // Header is a u32 (4 bytes)
            if ptr.len() < 4 {
                // FAIL FAST instead of breaking/truncating
                bail!("WAL corruption: Incomplete WAL frame header");
            }
            let mut p = ptr;
            let batch_size = p.get_u32() as usize;

            // Frame must contain the batch_size (body) + 4 bytes for the checksum footer
            if p.len() < batch_size + 4 {
                // FAIL FAST instead of breaking/truncating
                bail!("WAL corruption: Incomplete WAL frame body or checksum");
            }

            // Extract body and advance pointer
            let body_slice = &p[..batch_size];
            p.advance(batch_size);

            // Extract expected checksum and calculate actual checksum
            let expected_checksum = p.get_u32();
            let actual_checksum = crc32fast::hash(body_slice);

            if actual_checksum != expected_checksum {
                bail!("WAL corruption: Checksum mismatch");
            }

            // Parse body internally
            let mut body_ptr = body_slice;
            let mut batch_kvs = Vec::new();

            while !body_ptr.is_empty() {
                if body_ptr.len() < 2 { bail!("WAL corruption: Invalid nested key length"); }
                let key_len = body_ptr.get_u16() as usize;

                if body_ptr.len() < key_len { bail!("WAL corruption: Key bounds exceed body slice"); }
                let key_bytes = Bytes::copy_from_slice(&body_ptr[..key_len]);
                body_ptr.advance(key_len);

                if body_ptr.len() < 8 { bail!("WAL corruption: Missing timestamp"); }
                let ts = body_ptr.get_u64();

                if body_ptr.len() < 2 { bail!("WAL corruption: Invalid nested value length"); }
                let val_len = body_ptr.get_u16() as usize;

                if body_ptr.len() < val_len { bail!("WAL corruption: Value bounds exceed body slice"); }
                let val = Bytes::copy_from_slice(&body_ptr[..val_len]);
                body_ptr.advance(val_len);

                let key_slice = KeySlice::from_slice(&key_bytes, ts);
                batch_kvs.push((key_slice.to_key_vec().into_key_bytes(), val));
            }

            // Frame is perfectly valid. Commit to Skiplist.
            for (k, v) in batch_kvs {
                _skiplist.insert(k, v);
            }

            // Advance the main pointer to the next frame and track valid offset
            ptr = p;
            valid_offset += 4 + batch_size + 4;
        }

        // If the loop finished cleanly but the file has dangling incomplete bytes, truncate them
        if valid_offset < buf.len() {
            file.set_len(valid_offset as u64)?;
        }

        Ok(Wal {
            file: Arc::new(Mutex::new(BufWriter::new(file))),
        })
    }

    pub fn put(&self, _key: KeySlice, _value: &[u8]) -> Result<()> {
        self.put_batch(&[(_key, _value)])
    }

    pub fn put_batch(&self, _data: &[(KeySlice, &[u8])]) -> Result<()> {
        let mut body_size = 0_usize;

        // Verify bounds & calculate the required body size
        for (key, val) in _data {
            if key.key_len() > u16::MAX as usize || val.len() > u16::MAX as usize {
                bail!("Key or value size exceeds u16::MAX bounds");
            }
            body_size += 2 + key.key_len() + 8 + 2 + val.len();
        }

        if body_size > u32::MAX as usize {
            bail!("Total batch size exceeds u32::MAX");
        }

        // Capacity: 4 (batch_size header) + body_size + 4 (checksum footer)
        let mut buf = BytesMut::with_capacity(4 + body_size + 4);

        // 1. Write Header
        buf.put_u32(body_size as u32);

        // 2. Write Body
        for (key, val) in _data {
            buf.put_u16(key.key_len() as u16);
            buf.put_slice(key.key_ref());
            buf.put_u64(key.ts());
            buf.put_u16(val.len() as u16);
            buf.put_slice(val);
        }

        // 3. Calculate and append checksum over the exact body boundary
        let body_slice = &buf[4..4 + body_size];
        let checksum = crc32fast::hash(body_slice);
        buf.put_u32(checksum);

        // 4. Lock file writer and commit memory flush
        let mut writer = self.file.lock();
        writer.write_all(&buf.freeze())?;

        Ok(())
    }

    pub fn sync(&self) -> Result<()> {
        let mut writer = self.file.lock();
        writer.flush()?;
        writer.get_mut().sync_all()?;
        Ok(())
    }
}
