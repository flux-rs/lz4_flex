#[cfg(feature = "alloc")]
#[allow(unused_imports)]
use alloc::boxed::Box;

/// The Hashtable trait used by the compression to store hashed bytes to their position.
/// `val` can be maximum the size of the input in bytes.
///
/// `pos` can have a maximum value of u16::MAX or 65535
/// If the hashtable is smaller it needs to reduce the pos to its space, e.g. by right
/// shifting.
///
/// Duplication dictionary size.
///
/// Every four bytes is assigned an entry. When this number is lower, fewer entries exists, and
/// thus collisions are more likely, hurting the compression ratio.
///
/// hashes and right shifts to a maximum value of 16bit, 65535
/// The right shift is done in order to not exceed, the hashtables capacity
#[inline]
// CENTRIFUGE: `>> 16` -> `/ 65536` (same value; flux doesn't model `>>`). `strict`
// overflow mode tells flux the product fits in a u32.
#[flux_rs::opts(check_overflow = "strict")]
#[flux_rs::spec(fn(u32) -> u32{v: v < 65536})]
fn hash(sequence: u32) -> u32 {
    (sequence.wrapping_mul(2654435761_u32)) / 65536
}

/// hashes and right shifts to a maximum value of 16bit, 65535
/// The right shift is done in order to not exceed, the hashtables capacity
#[cfg(target_pointer_width = "64")]
#[inline]
#[flux_rs::opts(check_overflow = "strict")]
#[flux_rs::spec(fn(usize) -> u32{v: v < 65536})]
fn hash5(sequence: usize) -> u32 {
    let primebytes = if cfg!(target_endian = "little") {
        889523592379_usize
    } else {
        11400714785074694791_usize
    };
    // CENTRIFUGE: `>> 48` -> `/ (1 << 48)` (same value; flux doesn't model `>>`).
    (((sequence << 24).wrapping_mul(primebytes)) / 281_474_976_710_656) as u32
}

pub(crate) trait HashTable {
    #[flux_rs::spec(fn(&Self, pos: usize{pos < 65536}) -> usize)]
    fn get_at(&self, pos: usize) -> usize;
    #[flux_rs::spec(fn(&mut Self, pos: usize{pos < 65536}, val: usize))]
    fn put_at(&mut self, pos: usize, val: usize);
    fn clear(&mut self);
    #[inline]
    #[cfg(target_pointer_width = "64")]
    #[flux_rs::spec(fn(input: &[u8][@n], pos: usize{pos + 8 <= n}) -> usize{v: v < 65536})]
    fn get_hash_at(input: &[u8], pos: usize) -> usize {
        hash5(super::compress::get_batch_arch(input, pos)) as usize
    }
    #[inline]
    #[cfg(target_pointer_width = "32")]
    fn get_hash_at(input: &[u8], pos: usize) -> usize {
        hash(super::compress::get_batch(input, pos)) as usize
    }
}

const HASHTABLE_SIZE_4K: usize = 4 * 1024;
const HASHTABLE_BIT_SHIFT_4K: usize = 4;
// CENTRIFUGE: `get_at`/`put_at` index with `hash / 16` instead of `hash >> HASHTABLE_BIT_SHIFT_4K`
// (same value; flux doesn't model `>>`). Keep the two in sync:
const _: () = assert!(1 << HASHTABLE_BIT_SHIFT_4K == 16);

/// A 4K entry hash table using 16-bit values.
#[derive(Debug)]
#[repr(align(64))]
pub struct HashTable4KU16 {
    #[cfg(feature = "alloc")]
    dict: Box<[u16; HASHTABLE_SIZE_4K]>,
    #[cfg(not(feature = "alloc"))]
    dict: [u16; HASHTABLE_SIZE_4K],
}
impl HashTable4KU16 {
    /// Creates a new zeroed hash table.
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn new() -> Self {
        // This generates more efficient assembly in contrast to Box::new(slice), because of an
        // optimized call alloc_zeroed, vs. alloc + memset
        // try_into is optimized away
        let dict = alloc::vec![0; HASHTABLE_SIZE_4K]
            .into_boxed_slice()
            .try_into()
            .unwrap();
        Self { dict }
    }

    /// Creates a new zeroed hash table.
    #[cfg(not(feature = "alloc"))]
    #[inline]
    pub const fn new() -> Self {
        Self {
            dict: [0; HASHTABLE_SIZE_4K],
        }
    }
}
impl HashTable for HashTable4KU16 {
    #[inline]
    #[flux_rs::spec(fn(&Self, hash: usize{hash < 65536}) -> usize)]
    fn get_at(&self, hash: usize) -> usize {
        self.dict[hash / 16] as usize
    }
    #[inline]
    #[flux_rs::spec(fn(&mut Self, hash: usize{hash < 65536}, val: usize))]
    fn put_at(&mut self, hash: usize, val: usize) {
        self.dict[hash / 16] = val as u16;
    }
    #[inline]
    fn clear(&mut self) {
        self.dict.fill(0);
    }
    #[inline]
    #[flux_rs::spec(fn(input: &[u8][@n], pos: usize{pos + 8 <= n}) -> usize{v: v < 65536})]
    fn get_hash_at(input: &[u8], pos: usize) -> usize {
        hash(super::get_batch(input, pos)) as usize
    }
}

/// A 4K entry hash table using 32-bit values.
#[derive(Debug)]
pub struct HashTable4K {
    #[cfg(feature = "alloc")]
    dict: Box<[u32; HASHTABLE_SIZE_4K]>,
    #[cfg(not(feature = "alloc"))]
    dict: [u32; HASHTABLE_SIZE_4K],
}
impl HashTable4K {
    /// Creates a new zeroed hash table.
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn new() -> Self {
        let dict = alloc::vec![0; HASHTABLE_SIZE_4K]
            .into_boxed_slice()
            .try_into()
            .unwrap();
        Self { dict }
    }

    /// Creates a new zeroed hash table.
    #[cfg(not(feature = "alloc"))]
    #[inline]
    pub const fn new() -> Self {
        Self {
            dict: [0; HASHTABLE_SIZE_4K],
        }
    }

    /// Shifts all entries down by `offset`, clamping at zero.
    #[cold]
    #[allow(dead_code)]
    pub fn reposition(&mut self, offset: u32) {
        for i in self.dict.iter_mut() {
            *i = i.saturating_sub(offset);
        }
    }
}
impl HashTable for HashTable4K {
    #[inline]
    #[flux_rs::spec(fn(&Self, hash: usize{hash < 65536}) -> usize)]
    fn get_at(&self, hash: usize) -> usize {
        self.dict[hash / 16] as usize
    }
    #[inline]
    #[flux_rs::spec(fn(&mut Self, hash: usize{hash < 65536}, val: usize))]
    fn put_at(&mut self, hash: usize, val: usize) {
        self.dict[hash / 16] = val as u32;
    }
    #[inline]
    fn clear(&mut self) {
        self.dict.fill(0);
    }
}

const HASHTABLE_SIZE_8K: usize = 8 * 1024;
const HASH_TABLE_BIT_SHIFT_8K: usize = 3;
// CENTRIFUGE: `hash / 8` instead of `hash >> HASH_TABLE_BIT_SHIFT_8K` (same value). Keep in sync:
const _: () = assert!(1 << HASH_TABLE_BIT_SHIFT_8K == 8);

#[derive(Debug)]
pub(crate) struct HashTable8K {
    #[cfg(feature = "alloc")]
    dict: Box<[u32; HASHTABLE_SIZE_8K]>,
    #[cfg(not(feature = "alloc"))]
    dict: [u32; HASHTABLE_SIZE_8K],
}
#[allow(dead_code)]
impl HashTable8K {
    #[cfg(feature = "alloc")]
    #[inline]
    pub(crate) fn new() -> Self {
        let dict = alloc::vec![0; HASHTABLE_SIZE_8K]
            .into_boxed_slice()
            .try_into()
            .unwrap();

        Self { dict }
    }
    #[cfg(not(feature = "alloc"))]
    #[inline]
    pub(crate) const fn new() -> Self {
        Self {
            dict: [0; HASHTABLE_SIZE_8K],
        }
    }
}
impl HashTable for HashTable8K {
    #[inline]
    #[flux_rs::spec(fn(&Self, hash: usize{hash < 65536}) -> usize)]
    fn get_at(&self, hash: usize) -> usize {
        self.dict[hash / 8] as usize
    }
    #[inline]
    #[flux_rs::spec(fn(&mut Self, hash: usize{hash < 65536}, val: usize))]
    fn put_at(&mut self, hash: usize, val: usize) {
        self.dict[hash / 8] = val as u32;
    }
    #[inline]
    fn clear(&mut self) {
        self.dict.fill(0);
    }
}
