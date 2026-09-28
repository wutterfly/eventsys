use std::{collections::VecDeque, marker::PhantomData, sync::MutexGuard};

use crate::slot::{Slot, Store};

#[derive(Debug)]
/// An iterator over events from type `T`.
pub struct Query<'a, T> {
    events: MutexGuard<'a, Store<T>>,
}

impl<'a, T> Query<'a, T> {
    /// Creates a new `Query` to iterate over events from type `T`.
    #[inline]
    pub(crate) const fn new(events: MutexGuard<'a, Store<T>>) -> Self {
        Self { events }
    }

    #[inline]
    /// Returns the number of events this `Query` can produce.
    pub fn len(&self) -> usize {
        self.events.len()
    }
}

impl<T> Iterator for Query<'_, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.events.pop_front()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.events.len();
        (len, Some(len))
    }
}

impl<T> ExactSizeIterator for Query<'_, T> {}

impl<T> Drop for Query<'_, T> {
    #[inline]
    fn drop(&mut self) {
        self.events.clear();
    }
}

// ############################
// ############################
// ############################

#[derive(Debug)]
/// An iterator over events from type `T`.
pub struct UnblockingQuery<'a, T> {
    events: VecDeque<T>,

    /// The slot the events were taken from. Gets the buffer back, after all events are consumed.
    slot: &'a Slot<T>,

    _t: PhantomData<T>,
}

impl<'a, T> UnblockingQuery<'a, T> {
    #[inline]
    /// Creates a new `Query` to iterate over the events, that are currently stored in the slot.
    pub(crate) fn new(slot: &'a Slot<T>) -> Self {
        Self {
            events: slot.events_clone(),
            slot,
            _t: PhantomData,
        }
    }

    #[inline]
    /// Returns the number of events this `Query` can produce.
    pub fn len(&self) -> usize {
        self.events.len()
    }
}

impl<T> Iterator for UnblockingQuery<'_, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.events.pop_front()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.events.len();
        (len, Some(len))
    }
}

impl<T> ExactSizeIterator for UnblockingQuery<'_, T> {}

impl<T> Drop for UnblockingQuery<'_, T> {
    #[inline]
    fn drop(&mut self) {
        // drop all events that were not consumed
        self.events.clear();

        // the slot can use the buffer again
        self.slot.recycle(std::mem::take(&mut self.events));
    }
}
