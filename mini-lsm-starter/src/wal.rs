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

use crate::key::KeySlice;
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

    pub fn recover(_path: impl AsRef<Path>, _skiplist: &SkipMap<Bytes, Bytes>) -> Result<Self> {
        let mut file = File::options().read(true).append(true).open(_path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let mut ptr = buf.as_slice();

        while !ptr.is_empty() {
            if ptr.len() < 2 {
                break; // Incomplete key length header
            }
            let key_len = ptr.get_u16() as usize;
            if ptr.len() < key_len {
                break; // Incomplete key payload
            }
            let key = Bytes::copy_from_slice(&ptr[..key_len]);
            ptr.advance(key_len);

            if ptr.len() < 2 {
                break; // Incomplete value length header
            }
            let val_len = ptr.get_u16() as usize;
            if ptr.len() < val_len {
                break; // Incomplete value payload
            }
            let val = Bytes::copy_from_slice(&ptr[..val_len]);
            ptr.advance(val_len);

            _skiplist.insert(key, val);
        }

        Ok(Wal {
            file: Arc::new(Mutex::new(BufWriter::new(file))),
        })
    }

    pub fn put(&self, _key: &[u8], _value: &[u8]) -> Result<()> {
        if _key.len() > u16::MAX as usize || _value.len() > u16::MAX as usize {
            bail!("Key or value size exceeds u16::MAX");
        }
        let total_size = _key.len() + _value.len() + 4;
        let mut buf = BytesMut::with_capacity(total_size);

        buf.put_u16(_key.len() as u16);
        buf.put_slice(_key);
        buf.put_u16(_value.len() as u16);
        buf.put_slice(_value);

        let mut writer = self.file.lock();

        writer.write_all(&buf.freeze())?;

        Ok(())
    }

    /// Implement this in week 3, day 5.
    pub fn put_batch(&self, _data: &[(KeySlice, &[u8])]) -> Result<()> {
        unimplemented!()
    }

    pub fn sync(&self) -> Result<()> {
        let mut writer = self.file.lock();

        writer.flush()?;

        writer.get_mut().sync_all()?;

        Ok(())
    }
}
