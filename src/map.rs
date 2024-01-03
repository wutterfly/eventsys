use std::any::TypeId;

use crate::backend::Registered;

pub struct RegisteredMap<const SIZE: usize> {
    inner: Vec<(TypeId, Registered<SIZE>)>,
}

impl<const SIZE: usize> RegisteredMap<SIZE> {
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self { inner: Vec::new() }
    }

    #[inline]
    #[must_use]
    pub fn get(&self, key: &TypeId) -> Option<&Registered<SIZE>> {
        self.inner.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    #[inline]
    #[must_use]
    pub fn get_mut(&mut self, key: &TypeId) -> Option<&mut Registered<SIZE>> {
        self.inner
            .iter_mut()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    #[inline]
    #[must_use]
    pub fn insert(&mut self, key: TypeId, value: Registered<SIZE>) -> Option<Registered<SIZE>> {
        let found = self.inner.iter_mut().find(|(k, _)| k == &key);

        if let Some((_, v)) = found {
            let out = std::mem::replace(v, value);

            Some(out)
        } else {
            self.inner.push((key, value));
            None
        }
    }

    #[inline]
    pub fn values<'a>(&'a self) -> impl Iterator<Item = &'a Registered<SIZE>>
    where
        TypeId: 'a,
    {
        self.inner.iter().map(|(_, v)| v)
    }

    #[inline]
    pub fn values_mut<'a>(&'a mut self) -> impl Iterator<Item = &'a mut Registered<SIZE>>
    where
        Registered<SIZE>: 'a,
    {
        self.inner.iter_mut().map(|(_, v)| v)
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }
}
