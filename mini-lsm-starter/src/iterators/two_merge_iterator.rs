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

use anyhow::Result;

use super::StorageIterator;

/// Merges two iterators of different types into one. If the two iterators have the same key, only
/// produce the key once and prefer the entry from A.
pub struct TwoMergeIterator<A: StorageIterator, B: StorageIterator> {
    a: A,
    b: B,
    is_a: bool,
    is_valid: bool,
}

impl<
    A: 'static + StorageIterator,
    B: 'static + for<'a> StorageIterator<KeyType<'a> = A::KeyType<'a>>,
> TwoMergeIterator<A, B>
{
    pub fn create(a: A, b: B) -> Result<Self> {
        let mut iter = TwoMergeIterator {
            a,
            b,
            is_a: true,
            is_valid: true,
        };

        iter.skip_b_and_choose()?;
        Ok(iter)
    }

    /// Centralized logic to skip duplicates and choose the next iterator
    fn skip_b_and_choose(&mut self) -> Result<()> {
        // Guarantee B is skipped if it matches A, regardless of who just advanced
        while self.a.is_valid() && self.b.is_valid() && self.a.key() == self.b.key() {
            self.b.next()?;
        }

        if !self.a.is_valid() && !self.b.is_valid() {
            self.is_valid = false;
        } else if !self.a.is_valid() {
            self.is_a = false;
        } else if !self.b.is_valid() {
            self.is_a = true;
        } else if self.b.key() < self.a.key() {
            self.is_a = false;
        } else {
            self.is_a = true;
        }

        Ok(())
    }
}

impl<
    A: 'static + StorageIterator,
    B: 'static + for<'a> StorageIterator<KeyType<'a> = A::KeyType<'a>>,
> StorageIterator for TwoMergeIterator<A, B>
{
    type KeyType<'a> = A::KeyType<'a>;

    fn key(&self) -> Self::KeyType<'_> {
        if self.is_a {
            self.a.key()
        } else {
            self.b.key()
        }
    }

    fn value(&self) -> &[u8] {
        if self.is_a {
            self.a.value()
        } else {
            self.b.value()
        }
    }

    fn is_valid(&self) -> bool {
        self.is_valid
    }

    fn next(&mut self) -> Result<()> {
        if self.is_a {
            self.a.next()?;
        } else {
            self.b.next()?;
        }

        self.skip_b_and_choose()?;

        Ok(())
    }
}
