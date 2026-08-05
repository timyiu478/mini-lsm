* approximate_size
    * role: determine when to flush/freeze
    * calculation: the raw byte size of the key and value
    * why approximate?
        * omit the overhead of Arc, skiplist nodes, memory align padding, ...

* Storage Integration
    * To access the memtable, acquire the state lock. Because MemTable::put requires only an immutable reference (&self), you need only a read lock on state
    * Why? The SkipMap inside MemTable handles concurrent writes internally without requiring exclusive access to the LsmStorageState struct.


