#![deny(warnings)]
#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

//! `fixed_pool` implements an object pool with a fixed number of items. The items may be borrowed without lifetime restrictions,
//! are automatically returned to the pool upon drop, and may have customized reset semantics through the use of a trait.

use std::cell::*;
use std::marker::*;
use std::ops::*;
use std::sync::atomic::*;
use std::sync::*;

/// Allows for borrowing from a fixed pool of recycled values.
pub struct FixedPool<T, R: Reset<T> = NoopReset>(Arc<FixedPoolInner<T>>, PhantomData<fn() -> R>);

impl<T, R: Reset<T>> FixedPool<T, R> {
    /// Creates a pool which contains the given set of values.
    pub fn new(elements: impl IntoIterator<Item = T>) -> Self {
        let elements = elements
            .into_iter()
            .map(UnsafeCell::new)
            .collect::<Vec<_>>();
        let pulled_element_len = elements.len().saturating_sub(1) / usize::BITS as usize + 1;
        let mut pulled_elements = Vec::with_capacity(pulled_element_len);

        for _ in 0..pulled_element_len {
            pulled_elements.push(AtomicUsize::new(0));
        }

        let remaining_elements_len = elements.len() % usize::BITS as usize;
        if remaining_elements_len > 0 {
            if let Some(last) = pulled_elements.last_mut() {
                last.fetch_or(usize::MAX << remaining_elements_len, Ordering::AcqRel);
            }
        }

        Self(
            Arc::new(FixedPoolInner {
                pulled_elements,
                elements,
            }),
            PhantomData,
        )
    }

    /// Obtains a new value from the pool, or returns `None` if all elements are in use.
    pub fn pull(&self) -> Option<PoolBorrow<T, R>> {
        for (usize_index, value) in self.0.pulled_elements.iter().enumerate() {
            let mut present_value = value.load(Ordering::Acquire);
            let mut next_zero;
            while {
                next_zero = present_value.trailing_ones() as usize;
                present_value |= !((usize::MAX << 1) << next_zero);
                next_zero
            } < usize::BITS as usize
            {
                let mask = 1 << next_zero;
                if (value.fetch_or(mask, Ordering::AcqRel) & mask) == 0 {
                    return Some(PoolBorrow {
                        index: usize_index * usize::BITS as usize + next_zero,
                        pool: self.clone(),
                    });
                }
            }
        }

        None
    }
}

impl<T, R: Reset<T>> Clone for FixedPool<T, R> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
    }
}

impl<T: std::fmt::Debug, R: Reset<T>> std::fmt::Debug for FixedPool<T, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("FixedPool").finish()
    }
}

/// Holds the inner state for a fixed pool.
struct FixedPoolInner<T> {
    /// A bitset representing the elements which are presently in use.
    pub pulled_elements: Vec<AtomicUsize>,
    /// The set of elements.
    pub elements: Vec<UnsafeCell<T>>,
}

unsafe impl<T: Send> Send for FixedPoolInner<T> {}
unsafe impl<T: Sync> Sync for FixedPoolInner<T> {}

/// Represents an object which is borrowed from the fixed pool.
#[derive(Debug)]
pub struct PoolBorrow<T, R: Reset<T> = NoopReset> {
    /// The index in the pool of the borrowed element.
    index: usize,
    /// The pool itself.
    pool: FixedPool<T, R>,
}

impl<T, R: Reset<T>> PoolBorrow<T, R> {
    /// The index of the item within the pool.
    pub fn index(&self) -> usize {
        self.index
    }
}

impl<T, R: Reset<T>> Deref for PoolBorrow<T, R> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.pool.0.elements.get_unchecked(self.index).get() as *const _) }
    }
}

impl<T, R: Reset<T>> DerefMut for PoolBorrow<T, R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.pool.0.elements.get_unchecked(self.index).get() }
    }
}

impl<T, R: Reset<T>> Drop for PoolBorrow<T, R> {
    fn drop(&mut self) {
        unsafe {
            R::reset(&mut *self);
            let usize_index = self.index / usize::BITS as usize;
            let bit_index = self.index % usize::BITS as usize;
            self.pool
                .0
                .pulled_elements
                .get_unchecked(usize_index)
                .fetch_and(!(1 << bit_index), Ordering::AcqRel);
        }
    }
}

/// Determines how an object is reset when it is returned to the pool.
pub trait Reset<T> {
    /// Resets the provided value.
    fn reset(value: &mut T);
}

/// Does nothing when resetting an object.
#[derive(Copy, Clone, Debug)]
pub struct NoopReset;

impl<T> Reset<T> for NoopReset {
    fn reset(_: &mut T) {}
}
