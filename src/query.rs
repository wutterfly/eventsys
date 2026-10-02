use std::collections::VecDeque;

use crate::slot::Slot;

/// An iterator over events from type `T`, taken from the slot up front (a snapshot at construction time).
#[derive(Debug)]
pub enum UnblockingQuery<'a, T> {
    /// `All`, `AllFilter`, `Cmp`, `Max`, `First` — the whole batch, swapped out of the slot at construction. The
    /// buffer is handed back to the slot for reuse when this is dropped.
    Owned {
        events: VecDeque<T>,
        slot: &'a Slot<T>,
    },

    /// `Last` — the at-most-one pending event, already taken out of the slot at construction.
    Taken(Option<T>),
}

impl<'a, T> UnblockingQuery<'a, T> {
    #[inline]
    pub(crate) const fn owned(events: VecDeque<T>, slot: &'a Slot<T>) -> Self {
        Self::Owned { events, slot }
    }

    #[inline]
    pub(crate) const fn taken(value: Option<T>) -> Self {
        Self::Taken(value)
    }

    #[inline]
    /// Returns the number of events this `Query` can produce.
    pub fn len(&self) -> usize {
        match self {
            Self::Owned { events, .. } => events.len(),
            Self::Taken(value) => usize::from(value.is_some()),
        }
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Iterator for UnblockingQuery<'_, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Owned { events, .. } => events.pop_front(),
            Self::Taken(value) => value.take(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<T> ExactSizeIterator for UnblockingQuery<'_, T> {}

impl<T> Drop for UnblockingQuery<'_, T> {
    #[inline]
    fn drop(&mut self) {
        // drop all events that were not consumed, then hand the buffer back for reuse
        if let Self::Owned { events, slot } = self {
            events.clear();
            slot.recycle(std::mem::take(events));
        }
    }
}

#[cfg(test)]
impl<T> UnblockingQuery<'_, T> {
    /// Test-only: inspects the taken buffer's capacity, to check the recycling behavior of `All`/`AllFilter`/
    /// `Cmp`/`Max`/`First`. Not meaningful for `Last`, which never allocates a buffer.
    pub(crate) fn capacity(&self) -> usize {
        match self {
            Self::Owned { events, .. } => events.capacity(),
            Self::Taken(_) => 0,
        }
    }
}
