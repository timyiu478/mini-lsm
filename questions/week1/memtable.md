# Memtable

Q. Why doesn't the memtable provide a delete API?

Q. Does it make sense for the memtable to store all write operations instead of only the latest version of a key? For example, the user puts a->1, a->2, and a->3 into the same memtable.

Q. Is it possible to use other data structures as the memtable in LSM? What are the pros/cons of using the skiplist?

Q. Why do we need a combination of state and state_lock? Can we only use state.read() and state.write()?

Q. Why does the order to store and to probe the memtables matter? If a key appears in multiple memtables, which version should you return to the user?

Q. Is the memory layout of the memtable efficient / does it have good data locality? (Think of how Byte is implemented and stored in the skiplist...) What are the possible optimizations to make the memtable more efficient?

Q. So we are using parking_lot locks in this course. Is its read-write lock a fair lock? What might happen to the readers trying to acquire the lock if there is one writer waiting for existing readers to stop?

Q. After freezing the memtable, is it possible that some threads still hold the old LSM state and wrote into these immutable memtables? How does your solution prevent it from happening?

Q. There are several places that you might first acquire a read lock on state, then drop it and acquire a write lock (these two operations might be in different functions but they happened sequentially due to one function calls the other). How does it differ from directly upgrading the read lock to a write lock? Is it necessary to upgrade instead of acquiring and dropping and what is the cost of doing the upgrade?


# Reference

https://skyzh.github.io/mini-lsm/week1-01-memtable.html
