# Block

## Q. Before looking at BlockBuilder::add, write a formula for the encoded size after adding one key-value pair. Include the key length, value length, entry offset, and element count. Which bytes are paid once per block, and which are paid once per entry?

Formula: current total bytes + bytes per 1 entry

bytes are paid once per block:

* entry count

bytes are paid once per entry:

* key-value pair
* key length
* value length
* entry offset


## Q. What is the time complexity of seeking a key in the block?

* seek first/next: O(1)
* seek specific key: O(logn)

## Q. Where does the cursor stop when you seek a non-existent key in your implementation?

It will stop to the first key that >= non-existent key.

## Q. What endianness does your implementation use for numbers written to blocks?

The offsets, key length, value length, and the num_of_elements are in big-endian byte order.

https://docs.rs/bytes/latest/bytes/buf/trait.BufMut.html#method.put_u16

## Q. Can a block contain duplicated keys?

Yes. There is no key deduplciation logic in block building.

## Q. What happens if the user adds a key larger than the target block size?

If there is no key in the block, the key will be added. Otherwise, the key will NOT be added and return false to the user.

## Q. Is your implementation vulnerable to a maliciously constructed block? Could invalid input cause an out-of-bounds access or an out-of-memory condition?

Yes. If the actaul values of the length fields such as number of entries and value length are larger than the expected values of the length fields, it can cause out-of-bounds access.

## Q. Construct three malformed blocks: one with an impossible entry count, one with a non-monotonic or out-of-range offset, and one with a length that extends beyond the data section. Where would the current decoder fail for each input, and what validation would reject it cleanly?

See the decode function comments.

## Q. Block is simply a vector of raw data and a vector of offsets. Could we change them to Bytes and Arc<[u16]>, then change the iterator interfaces to return Bytes instead of &[u8]? Assume that we use Bytes::slice to return a slice without copying. What are the advantages and disadvantages?


## Q. Suppose the LSM engine uses an object-storage service such as S3. How would you adapt the block format and its parameters to suit that environment?


