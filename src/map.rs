use std::any::TypeId;

use crate::backend::Registered;

/// Maps event types to their registration.
///
/// Keys and values are stored in separate vectors, so looking up a type only scans densely packed keys.
pub struct RegisteredMap<const SIZE: usize> {
    keys: Vec<TypeId>,
    values: Vec<Registered<SIZE>>,
}

impl<const SIZE: usize> RegisteredMap<SIZE> {
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            keys: Vec::new(),
            values: Vec::new(),
        }
    }

    #[inline]
    fn position(&self, key: &TypeId) -> Option<usize> {
        self.keys.iter().position(|k| k == key)
    }

    #[inline]
    #[must_use]
    pub fn get(&self, key: &TypeId) -> Option<&Registered<SIZE>> {
        self.position(key).map(|i| &self.values[i])
    }

    #[inline]
    #[must_use]
    pub fn get_mut(&mut self, key: &TypeId) -> Option<&mut Registered<SIZE>> {
        self.position(key).map(|i| &mut self.values[i])
    }

    #[inline]
    #[must_use]
    pub fn insert(&mut self, key: TypeId, value: Registered<SIZE>) -> Option<Registered<SIZE>> {
        if let Some(i) = self.position(&key) {
            Some(std::mem::replace(&mut self.values[i], value))
        } else {
            self.keys.push(key);
            self.values.push(value);
            None
        }
    }

    #[inline]
    pub fn values(&self) -> impl Iterator<Item = &Registered<SIZE>> {
        self.values.iter()
    }

    #[inline]
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Registered<SIZE>> {
        self.values.iter_mut()
    }

    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.keys.len()
    }
}
