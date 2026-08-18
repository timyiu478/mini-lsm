# Timestamp Encoding

## Q. In what order should a@7, a@3, aa@9, and b@1 appear? If a block entry shares all user-key bytes with the previous entry, which fields are still encoded for that entry?

Appear Order: a@7, a@3, aa@9, b@1

which fields are still encoded for that entry: key overlap length, remaining length, remain bytes (empty), timestamp, value length, value bytes

## Q. Why is timestamp order reversed while user-key order is not?

Because higher timestamps come first, performing a standard binary search for (user_key, search_ts) naturally lands on the highest timestamp <= search ts (the newest valid version visible to that snapshot).

## Q. Why should a point lookup for k test one user-key fingerprint rather than a separate fingerprint for every possible k@ts?

Because a lookup asks whether any version of that user key may exist.

## Q. Construct a seek target that distinguishes comparing full internal keys from comparing only user keys.

Suppose an SSTable block contains two versions of the key "apple":

("apple", ts = 20)

("apple", ts = 10)

Seek target: ("apple", ts = 15)

Full Internal Key Comparison:

Because timestamps sort in descending order, ("apple", ts = 20) is strictly less than ("apple", ts = 15). Seeking to ("apple", ts = 15) skips entry #1 and lands on entry #2 ("apple", ts = 10).

User-Key Only Comparison:

Comparing only user keys evaluates "apple" == "apple". The seek stops at entry #1 ("apple", ts = 20).

## Q. Which encoded structures would become inconsistent if block metadata omitted timestamps?

## Q. During Day 1, why is it acceptable for LsmIterator to return repeated user keys, and why must that behavior change on Day 2?
