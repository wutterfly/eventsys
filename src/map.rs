use std::{any::TypeId, panic::RefUnwindSafe};

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
struct Erased(anythingy::Thing<REGISTERED_SIZE>);

// SAFETY: `Erased` only ever holds a `Registered<T>` for some `T: Send + RefUnwindSafe + 'static` (the only thing
// `Entry::new` ever puts into it). `Registered<T>` is `Sync` for any such `T`:
//   - its `listener: Vec<Box<dyn Fn(&T) + Sync + ...>>` is `Sync` because the trait object itself requires `Sync`,
//   - its `slot: Option<Slot<T>>` wraps all storage in a `Mutex`, which provides `Sync` given only `T: Send`,
//   - `Slot<T>`'s `Cmp`/`AllFilter` variants hold plain `fn` pointers, which are `Sync` regardless of `T`.
// `anythingy::Thing` itself does not implement `Sync` (it holds its bytes in an `UnsafeCell`), because it cannot
// know whether an arbitrary caller's erased value is safe to share across threads. Here it is: every value ever
// erased into an `Erased` is a `Registered<T>`, and reading it from multiple threads via `get_ref` is exactly as
// sound as reading a `&Registered<T>` normally would be.
unsafe impl Sync for Erased {}

impl Erased {
    #[inline]
    fn new<T>(value: Registered<T>) -> Self
    where
        T: Send + RefUnwindSafe + 'static,
    {
        Self(anythingy::Thing::new(value))
    }

    #[inline]
    fn get_ref<T>(&self) -> &Registered<T>
    where
        T: Send + RefUnwindSafe + 'static,
    {
        self.0.get_ref()
    }

    #[inline]
    fn get_mut<T>(&mut self) -> &mut Registered<T>
    where
        T: Send + RefUnwindSafe + 'static,
    {
        self.0.get_mut()
    }
}

/// A registration, plus the operations needed to work on it without knowing its `T`.
///
/// `enable`/`disable`/`cleanup` are plain (non-capturing) function pointers, monomorphized once per `T` at
/// registration time; calling one just reconstructs the `&Registered<T>`/`&mut Registered<T>` and calls the
/// matching inherent method.
struct Entry {
    value: Erased,
    enable: fn(&Erased),
    disable: fn(&Erased),
    cleanup: fn(&mut Erased),
}

impl Entry {
    #[inline]
    fn new<T>() -> Self
    where
        T: Send + RefUnwindSafe + 'static,
    {
        Self {
            value: Erased::new(Registered::<T>::new()),
            enable: |erased| erased.get_ref::<T>().enable(),
            disable: |erased| erased.get_ref::<T>().disable(),
            cleanup: |erased| erased.get_mut::<T>().cleanup(),
        }
    }
}

/// Maps event types to their registration.
///
/// Keys and values are stored in separate vectors, so looking up a type only scans densely packed keys. Each
/// value is type-erased into a fixed-size inline buffer (see [`REGISTERED_SIZE`]), so registering a type never
/// needs a separate heap allocation.
pub struct RegisteredMap {
    keys: Vec<TypeId>,
    entries: Vec<Entry>,
}

impl RegisteredMap {
    #[inline]
    pub const fn new() -> Self {
        Self {
            keys: Vec::new(),
            entries: Vec::new(),
        }
    }

    #[inline]
    fn position(&self, key: &TypeId) -> Option<usize> {
        self.keys.iter().position(|k| k == key)
    }

    /// Returns the registration for `T`, if one exists.
    #[inline]
    pub fn get<T>(&self) -> Option<&Registered<T>>
    where
        T: Send + RefUnwindSafe + 'static,
    {
        self.position(&TypeId::of::<T>())
            .map(|i| self.entries[i].value.get_ref::<T>())
    }

    /// Returns the registration for `T`, creating an empty one if none exists yet.
    #[inline]
    pub fn entry<T>(&mut self) -> &mut Registered<T>
    where
        T: Send + RefUnwindSafe + 'static,
    {
        let id = TypeId::of::<T>();

        let i = self.position(&id).unwrap_or_else(|| {
            self.keys.push(id);
            self.entries.push(Entry::new::<T>());
            self.entries.len() - 1
        });

        self.entries[i].value.get_mut::<T>()
    }

    #[inline]
    pub fn disable_all(&self) {
        for entry in &self.entries {
            (entry.disable)(&entry.value);
        }
    }

    #[inline]
    pub fn enable_all(&self) {
        for entry in &self.entries {
            (entry.enable)(&entry.value);
        }
    }

    #[inline]
    pub fn cleanup_all(&mut self) {
        for entry in &mut self.entries {
            (entry.cleanup)(&mut entry.value);
        }
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.keys.len()
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
        assert_eq!(size_of::<Registered<Box<dyn Send + Sync>>>(), REGISTERED_SIZE);
    }

    #[test]
    fn test_registered_fits_without_boxing() {
        assert!(anythingy::Thing::<REGISTERED_SIZE>::fitting::<Registered<()>>());
        assert!(anythingy::Thing::<REGISTERED_SIZE>::fitting::<Registered<u8>>());
        assert!(anythingy::Thing::<REGISTERED_SIZE>::fitting::<Registered<Big>>());
        assert!(!anythingy::Thing::<REGISTERED_SIZE>::boxed::<Registered<Big>>());
    }
}
