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

use std::collections::{BTreeMap, VecDeque, btree_map::Entry};

pub struct Watermark {
    /// Queue storing unique active timestamps in chronological order
    deque: VecDeque<u64>,
    /// HashMap mapping timestamp -> active_reader_count for O(1) removals
    readers: BTreeMap<u64, usize>,
}

impl Default for Watermark {
    fn default() -> Self {
        Self::new()
    }
}

impl Watermark {
    pub fn new() -> Self {
        Self {
            deque: VecDeque::new(),
            readers: BTreeMap::new(),
        }
    }

    pub fn add_reader(&mut self, ts: u64) {
        let count = self.readers.entry(ts).or_insert(0);
        if *count == 0 {
            self.deque.push_back(ts);
        }
        *count += 1;
    }

    pub fn remove_reader(&mut self, ts: u64) {
        if let Entry::Occupied(mut entry) = self.readers.entry(ts) {
            *entry.get_mut() -= 1;
            if *entry.get() == 0 {
                entry.remove();
            }
        }

        while let Some(&front_ts) = self.q.front() {
            if self.readers.contains_key(&front_ts) {
                break;
            }
            self.deque.pop_front();
        }
    }

    pub fn num_retained_snapshots(&self) -> usize {
        self.readers.len()
    }

    pub fn watermark(&self) -> Option<u64> {
        self.deque.front().copied()
    }
}
