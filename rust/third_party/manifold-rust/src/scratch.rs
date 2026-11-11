// scratch.rs — recycling of the large buffers of mesh operations
//
// A boolean on a mesh of half a million triangles allocates a few hundred
// megabytes, in buffers of megabytes each: the arrays of the result and the
// work arrays of every stage. A system allocator hands a buffer of that size
// straight to the operating system when it is freed and asks for fresh memory
// for the next one, and fresh memory is slow to touch for the first time: each
// 4 KB page takes a page fault to map and zero. On such a mesh that was about
// a fifth of the time of a boolean.
//
// So large buffers are kept here when their owner is done with them (work
// arrays as they go out of scope, a mesh's arrays when the mesh is dropped)
// and handed out again to whoever next asks for one of the same element type.
// Only capacity is recycled: a buffer comes back empty, so no value can leak
// from one use to the next and no result depends on the pool.
//
// The pool holds at most `limit()` bytes, the oldest buffers leaving first.
// `set_limit(0)` turns it off.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Buffers smaller than this are left to the allocator, which keeps small
/// blocks on its own free lists.
const MIN_BYTES: usize = 256 * 1024;

/// The default for [`limit`].
#[cfg(not(target_arch = "wasm32"))]
pub const DEFAULT_LIMIT: usize = 512 * 1024 * 1024;

/// The default for [`limit`]. Smaller on wasm32: its linear memory only grows,
/// so what the pool holds can never go back to the browser, and the whole
/// address space is 4 GB.
#[cfg(target_arch = "wasm32")]
pub const DEFAULT_LIMIT: usize = 64 * 1024 * 1024;

static LIMIT: AtomicUsize = AtomicUsize::new(DEFAULT_LIMIT);

struct Entry {
    bytes: usize,
    /// Capacity in elements.
    capacity: usize,
    /// Order of arrival, for eviction.
    seq: u64,
    /// A `Vec<T>`, empty.
    buffer: Box<dyn Any + Send>,
}

#[derive(Default)]
struct Pool {
    held: usize,
    next_seq: u64,
    by_type: HashMap<TypeId, Vec<Entry>>,
}

static POOL: Mutex<Option<Pool>> = Mutex::new(None);

fn with_pool<R>(f: impl FnOnce(&mut Pool) -> R) -> R {
    // A panic while the lock is held cannot leave the pool inconsistent in a
    // way that matters (it only holds empty buffers), so a poisoned lock is
    // used as it is.
    let mut guard = POOL.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Pool::default))
}

/// The most bytes the pool keeps.
pub fn limit() -> usize {
    LIMIT.load(Ordering::Relaxed)
}

/// Set the most bytes the pool keeps; buffers beyond it are freed now. `0`
/// turns recycling off.
pub fn set_limit(bytes: usize) {
    LIMIT.store(bytes, Ordering::Relaxed);
    with_pool(|pool| pool.evict_to(bytes));
}

/// Free every buffer the pool holds.
pub fn clear() {
    with_pool(|pool| pool.evict_to(0));
}

/// Bytes the pool holds right now.
pub fn held_bytes() -> usize {
    with_pool(|pool| pool.held)
}

impl Pool {
    /// Drop the oldest buffers until at most `bytes` are held.
    fn evict_to(&mut self, bytes: usize) {
        while self.held > bytes {
            let oldest = self
                .by_type
                .iter()
                .filter_map(|(ty, list)| list.iter().map(|e| e.seq).min().map(|seq| (seq, *ty)))
                .min();
            let Some((seq, ty)) = oldest else {
                self.held = 0;
                return;
            };
            if let Some(list) = self.by_type.get_mut(&ty) {
                if let Some(at) = list.iter().position(|e| e.seq == seq) {
                    self.held -= list.swap_remove(at).bytes;
                }
            }
        }
    }
}

/// The pooled buffer of `T` that fits `capacity` best, if one has at least
/// that capacity and no more than `most`.
fn take_between<T: Send + 'static>(capacity: usize, most: usize) -> Option<Vec<T>> {
    if capacity.saturating_mul(std::mem::size_of::<T>()) < MIN_BYTES || limit() == 0 {
        return None;
    }
    let boxed = with_pool(|pool| {
        let list = pool.by_type.get_mut(&TypeId::of::<T>())?;
        let at = list
            .iter()
            .enumerate()
            .filter(|(_, e)| e.capacity >= capacity && e.capacity <= most)
            .min_by_key(|(_, e)| e.capacity)
            .map(|(at, _)| at)?;
        let entry = list.swap_remove(at);
        pool.held -= entry.bytes;
        Some(entry.buffer)
    })?;
    boxed.downcast::<Vec<T>>().ok().map(|v| *v)
}

/// An empty vector with room for `capacity` elements, for an array that stays
/// in a mesh: a recycled buffer is used only if it is at most a quarter larger
/// than asked for, so a mesh does not sit on memory it has no use for.
pub fn take<T: Send + 'static>(capacity: usize) -> Vec<T> {
    take_between(capacity, capacity + capacity / 4).unwrap_or_else(|| Vec::with_capacity(capacity))
}

/// [`take`], filled with `len` copies of `value`.
pub fn filled<T: Clone + Send + 'static>(len: usize, value: T) -> Vec<T> {
    let mut v = take(len);
    v.resize(len, value);
    v
}

/// A copy of `slice` in a recycled buffer.
pub fn copy_of<T: Clone + Send + 'static>(slice: &[T]) -> Vec<T> {
    let mut v = take(slice.len());
    v.extend_from_slice(slice);
    v
}

/// Hand a vector that is no longer needed to the pool. Its elements are
/// dropped here; only its capacity is kept.
pub fn give<T: Send + 'static>(mut v: Vec<T>) {
    let bytes = v.capacity().saturating_mul(std::mem::size_of::<T>());
    let limit = limit();
    if bytes < MIN_BYTES || bytes > limit {
        return;
    }
    v.clear();
    let capacity = v.capacity();
    with_pool(|pool| {
        pool.evict_to(limit - bytes);
        let seq = pool.next_seq;
        pool.next_seq += 1;
        pool.held += bytes;
        pool.by_type.entry(TypeId::of::<T>()).or_default().push(Entry {
            bytes,
            capacity,
            seq,
            buffer: Box::new(v),
        });
    });
}

/// [`give`] for a vector that stays where it is: it is left empty.
pub fn give_from<T: Send + 'static>(v: &mut Vec<T>) {
    give(std::mem::take(v));
}

/// A work array that goes back to the pool when it is dropped. It derefs to
/// its `Vec<T>`.
pub struct Scratch<T: Send + 'static>(Vec<T>);

impl<T: Send + 'static> Scratch<T> {
    /// An empty work array with room for `capacity` elements. Unlike [`take`]
    /// it accepts a recycled buffer several times larger than asked for: it
    /// goes straight back to the pool.
    pub fn with_capacity(capacity: usize) -> Self {
        Scratch(
            take_between(capacity, capacity.saturating_mul(4).saturating_add(1 << 20))
                .unwrap_or_else(|| Vec::with_capacity(capacity)),
        )
    }

    /// A work array of `len` copies of `value`.
    pub fn filled(len: usize, value: T) -> Self
    where
        T: Clone,
    {
        let mut s = Self::with_capacity(len);
        s.0.resize(len, value);
        s
    }

    /// Make a work array of a vector, so that it is recycled when dropped.
    pub fn from_vec(v: Vec<T>) -> Self {
        Scratch(v)
    }

    /// The vector, no longer recycled when dropped.
    pub fn into_vec(mut self) -> Vec<T> {
        std::mem::take(&mut self.0)
    }
}

impl<T: Send + 'static> Deref for Scratch<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Vec<T> {
        &self.0
    }
}

impl<T: Send + 'static> DerefMut for Scratch<T> {
    fn deref_mut(&mut self) -> &mut Vec<T> {
        &mut self.0
    }
}

impl<T: Send + 'static> Drop for Scratch<T> {
    fn drop(&mut self) {
        give_from(&mut self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The pool is process-wide and other tests use it at the same time, so
    // these only check what holds whatever else is in it.

    #[test]
    fn a_given_buffer_comes_back_empty() {
        // An element type no other code uses, so the buffer is ours.
        #[derive(Clone)]
        struct Marker(#[allow(dead_code)] [u64; 4]);
        let n = MIN_BYTES; // well over the minimum in bytes
        let v = filled(n, Marker([7; 4]));
        let address = v.as_ptr() as usize;
        give(v);
        let back: Vec<Marker> = take(n);
        assert!(back.is_empty());
        assert!(back.capacity() >= n);
        if limit() >= n * std::mem::size_of::<Marker>() {
            assert_eq!(back.as_ptr() as usize, address);
        }
    }

    #[test]
    fn small_buffers_are_not_pooled() {
        #[derive(Clone)]
        struct Small(#[allow(dead_code)] u8);
        give(vec![Small(1); 16]);
        assert!(take_between::<Small>(16, usize::MAX).is_none());
    }

    #[test]
    fn a_much_larger_buffer_is_not_used_for_a_mesh_array() {
        #[derive(Clone)]
        struct Wide(#[allow(dead_code)] [u64; 8]);
        let n = MIN_BYTES;
        give(filled(4 * n, Wide([0; 8])));
        let v: Vec<Wide> = take(n);
        assert!(v.capacity() < 4 * n);
        // A work array may use it.
        let s: Scratch<Wide> = Scratch::with_capacity(n);
        assert!(s.capacity() >= n);
    }
}
