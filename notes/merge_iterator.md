## Q. Why separate current from iters?


```
/// Merge multiple iterators of the same type. If the same key occurs multiple times in some
/// iterators, prefer the one with smaller index.
pub struct MergeIterator<I: StorageIterator> {
    iters: BinaryHeap<HeapWrapper<I>>,
    current: Option<HeapWrapper<I>>,
}
```

By keeping the active iterator outside the heap, the logic for handling duplicate keys becomes straightforward:

If iters.peek().key() == current.key(), we know it is an older duplicate. We can just repeatedly call .next() on that heap iterator until the duplicate keys are cleared out, all without ever touching or disturbing current.
