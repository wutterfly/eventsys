use std::{any::TypeId, panic::RefUnwindSafe};

use anythingy::{HeapSize, InlineMap, TypeIdBuildHasher};

use crate::backend::Registered;

/// `Registered<T>` never stores a `T` value directly: its listeners live behind `Box<dyn Fn(&T) + ...>`
/// (pointer-sized, regardless of `T`), its slot's storage lives behind `VecDeque<T>` (also pointer-sized), and its
/// `Cmp`/`AllFilter` functions are plain `fn` pointers. So `Registered<T>`'s size and alignment are the same for
/// every `T`; `()` is just a convenient stand-in to compute that constant.
///
/// See `test_registered_size_is_type_independent` for a check that this actually holds.
const REGISTERED_SIZE: usize = size_of::<Registered<()>>();

/// A `Registered<T>`, type-erased into a fixed-size inline byte buffer, so storing one never needs a separate
/// heap allocation (`Registered<T>` always fits, see [`REGISTERED_SIZE`]).
type Erased = anythingy::SThing<REGISTERED_SIZE>;

/// A registration, plus the operations needed to work on it without knowing its `T`.
///
/// `enable`/`disable`/`clear`/`reset`/`heap_size` are plain (non-capturing) function pointers, monomorphized once
/// per `T` at registration time; calling one just reconstructs the `&Registered<T>`/`&mut Registered<T>` and calls
/// the matching inherent method.
struct Entry {
    value: Erased,
    enable: fn(&Erased),
    disable: fn(&Erased),
    clear: fn(&mut Erased),
    reset: fn(&mut Erased),
    heap_size: fn(&Erased) -> usize,
}

impl Entry {
    #[inline]
    fn new<T>() -> Self
    where
        T: Send + RefUnwindSafe + 'static,
    {
        Self {
            value: Erased::new(Registered::<T>::new()),
            enable: |erased| erased.get_ref::<Registered<T>>().enable(),
            disable: |erased| erased.get_ref::<Registered<T>>().disable(),
            clear: |erased| erased.get_mut::<Registered<T>>().clear(),
            reset: |erased| erased.get_mut::<Registered<T>>().reset(),
            // `erased` itself is only heap-allocated if `Registered<T>` didn't fit inline, which it always does
            // (see `REGISTERED_SIZE`), but it costs nothing to ask rather than assume.
            heap_size: |erased| erased.heap_size() + erased.get_ref::<Registered<T>>().heap_size(),
        }
    }
}

/// How many event types are looked up by scanning, before they are looked up by hashing instead.
///
/// As long as there are at most this many event types, they are found by comparing them to the registered ones.
/// Finding the first costs about `2.8 ns`, and every one it has to pass after that about `0.4 ns` more. Beyond this
/// many types, all of them are found by hashing, which costs about `3.3 ns` however many there are.
///
/// So hashing is already cheaper than scanning from about 3 types on, which is why this is small. Measured on the
/// whole benchmark suite: `4` and `8` are the same, `2` is a little slower for a frame that is mostly lookups, and
/// `16` is slower for a program with 16 event types, since it scans most of them for every event. `4` is the
/// smallest of the two that are the same, and every inline registration is part of the [`RegisteredMap`]. Within
/// this many types, register the ones that are used the most first, they are the cheapest to find.
const INLINE_TYPES: usize = 4;

/// Maps event types to their registration.
///
/// Up to [`INLINE_TYPES`] event types are found by scanning densely packed keys, and more than that with a hash map,
/// so finding a type does not get slower with every one that is registered. Each value is type-erased into a
/// fixed-size inline buffer (see [`REGISTERED_SIZE`]), so registering a type never needs a separate heap allocation
/// for the registration itself.
pub struct RegisteredMap {
    entries: InlineMap<TypeId, Entry, INLINE_TYPES, TypeIdBuildHasher>,
}

impl RegisteredMap {
    #[inline]
    pub const fn new() -> Self {
        Self {
            entries: InlineMap::with_hasher(TypeIdBuildHasher::new()),
        }
    }

    /// Returns the registration for `T`, if one exists.
    #[inline]
    pub fn get<T>(&self) -> Option<&Registered<T>>
    where
        T: Send + RefUnwindSafe + 'static,
    {
        self.entries
            .get(&TypeId::of::<T>())
            .map(|entry| entry.value.get_ref::<Registered<T>>())
    }

    /// Returns the registration for `T`, creating an empty one if none exists yet.
    #[inline]
    pub fn entry<T>(&mut self) -> &mut Registered<T>
    where
        T: Send + RefUnwindSafe + 'static,
    {
        self.entries
            .entry(TypeId::of::<T>())
            .or_insert_with(Entry::new::<T>)
            .value
            .get_mut::<Registered<T>>()
    }

    #[inline]
    pub fn disable_all(&self) {
        for entry in self.entries.values() {
            (entry.disable)(&entry.value);
        }
    }

    #[inline]
    pub fn enable_all(&self) {
        for entry in self.entries.values() {
            (entry.enable)(&entry.value);
        }
    }

    #[inline]
    pub fn clear_all(&mut self) {
        for entry in self.entries.values_mut() {
            (entry.clear)(&mut entry.value);
        }
    }

    #[inline]
    pub fn reset_all(&mut self) {
        for entry in self.entries.values_mut() {
            (entry.reset)(&mut entry.value);
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Counts the heap memory every registered type has allocated: the table of the types that do not fit inline,
    /// plus what each registration owns — see [`Registered::heap_size`] and [`crate::slot::Slot::heap_size`] for
    /// what that includes. Like [`anythingy::EventQueue::heap_size`], which some slots forward to, this walks every
    /// registration, so it is meant for occasional checks, and the result is a snapshot.
    #[inline]
    pub fn heap_size(&self) -> usize {
        self.entries.heap_size()
            + self
                .entries
                .values()
                .map(|entry| (entry.heap_size)(&entry.value))
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::{REGISTERED_SIZE, Registered};

    /// A deliberately huge, oddly-aligned type, to check that `Registered<T>`'s size really does not depend on `T`.
    #[repr(align(32))]
    #[allow(dead_code)]
    struct Big([u128; 50]);

    #[test]
    fn test_registered_size_is_type_independent() {
        assert_eq!(size_of::<Registered<()>>(), REGISTERED_SIZE);
        assert_eq!(size_of::<Registered<u8>>(), REGISTERED_SIZE);
        assert_eq!(size_of::<Registered<Big>>(), REGISTERED_SIZE);
        assert_eq!(
            size_of::<Registered<Box<dyn Send + Sync>>>(),
            REGISTERED_SIZE
        );
    }

    #[test]
    fn test_registered_fits_without_boxing() {
        assert!(anythingy::Thing::<REGISTERED_SIZE>::fitting::<Registered<()>>());
        assert!(anythingy::Thing::<REGISTERED_SIZE>::fitting::<Registered<u8>>());
        assert!(anythingy::Thing::<REGISTERED_SIZE>::fitting::<
            Registered<Big>,
        >());
        assert!(!anythingy::Thing::<REGISTERED_SIZE>::boxed::<Registered<Big>>());
    }
}
