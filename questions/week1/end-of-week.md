# Questions

## Q. the central correctness invariant of each component

* memtable: strict reverse chronological search order (newest MemTable ID to oldest MemTable ID). Newer writes shadow older ones.
* merge iterator: maintain global key order across multiple child iterators while resolving key collisions by yielding only the newest version and discarding older duplicates.
* sst block: entries are strictly sorted by key and immutable
* sst iterator: provide a seamless, sorted single-iterator view over an entire SST while bounding memory usage by lazily loading and holding only active block iterators in memory.

## Q. how the read and write paths choose the newest visible value for a key.

Read: 

* traversal order: newest MemTable ID to oldest MemTable ID -> L0 SSTs
* the search returns the first version encountered

Write:

* writes always go directly to the active MemTable.
* delete: put(b"")

## Q. The mutable memtable contains b -> delete and d -> 4. The newest immutable memtable contains a -> 1 and b -> 2. L0, from newest to oldest, contains one SST with a -> 0, c -> 3, and d -> 3. What do get(a), get(b), and the inclusive scan [a, d] return?

* get(a): 1
* get(b): none
* scan([a, d]): a->1, c->3, d->4

## Q. The immutable-memtable IDs are [7, 6, 5] from newest to oldest, and L0 IDs are [4, 3] from newest to oldest. After one correct flush, what are the two lists? Which logical read result is allowed to change because of the flush?

After one correct flush, The immutable-memtable IDs are [7, 6] from newest to olders, and L0 IDs are [5, 4, 3] from newest to oldest.

No logical read result is allowed to change because of the flush.

## Q. An SST’s range contains k, but its Bloom filter reports “may contain,” and an SST seek lands on the next key m. May get(k) return m? Would the answer change if the Bloom filter reported “definitely absent”?

No. Point lookups (get) require strict key equality (entry.key == k). If a seek inside an SST lands on key m where $m > k$, it proves key $k$ does not exist in that SST segment. get(k) evaluates this mismatch and returns None for that layer.

The answer does not change if the Bloom filter reported “definitely absent”, the get function will return None without searching the SSTs.
