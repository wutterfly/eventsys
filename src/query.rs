use std::{collections::VecDeque, marker::PhantomData, sync::MutexGuard};

use crate::{
    backend::Event,
    slot::{Slot, Store},
};

#[derive(Debug)]
/// An iterator over events from type `T`.
pub struct Query<'a, T, const EVENT_SIZE: usize>
where
    T: 'static,
{
    events: MutexGuard<'a, Store<EVENT_SIZE>>,

    _t: PhantomData<T>,
}

impl<'a, T, const EVENT_SIZE: usize> Query<'a, T, EVENT_SIZE>
where
    T: 'static,
{
    /// Creates a new `Query` to iterate over events from type `T`.
    #[inline]
    pub(crate) const fn new(events: MutexGuard<'a, Store<EVENT_SIZE>>) -> Self {
        Self {
            events,
            _t: PhantomData,
        }
    }

    #[inline]
    /// Returns the number of events this `Query` can produce.
    pub fn len(&self) -> usize {
        self.events.len()
    }
}

impl<T, const EVENT_SIZE: usize> Iterator for Query<'_, T, EVENT_SIZE>
where
    T: 'static,
{
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let out = self.events.pop_front();

        out.map(Event::get)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.events.len();
        (len, Some(len))
    }
}

impl<T, const EVENT_SIZE: usize> ExactSizeIterator for Query<'_, T, EVENT_SIZE> where T: 'static {}

impl<T, const EVENT_SIZE: usize> Drop for Query<'_, T, EVENT_SIZE>
where
    T: 'static,
{
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
pub struct UnblockingQuery<'a, T, const EVENT_SIZE: usize>
where
    T: 'static,
{
    events: VecDeque<Event<EVENT_SIZE>>,

    /// The slot the events were taken from. Gets the buffer back, after all events are consumed.
    slot: &'a Slot<EVENT_SIZE>,

    _t: PhantomData<T>,
}

impl<'a, T, const EVENT_SIZE: usize> UnblockingQuery<'a, T, EVENT_SIZE>
where
    T: 'static,
{
    #[inline]
    /// Creates a new `Query` to iterate over the events, that are currently stored in the slot.
    pub(crate) fn new(slot: &'a Slot<EVENT_SIZE>) -> Self {
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

impl<T, const EVENT_SIZE: usize> Iterator for UnblockingQuery<'_, T, EVENT_SIZE>
where
    T: 'static,
{
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let out = self.events.pop_front();

        out.map(Event::get)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.events.len();
        (len, Some(len))
    }
}

impl<T, const EVENT_SIZE: usize> ExactSizeIterator for UnblockingQuery<'_, T, EVENT_SIZE> where
    T: 'static
{
}

impl<T, const EVENT_SIZE: usize> Drop for UnblockingQuery<'_, T, EVENT_SIZE>
where
    T: 'static,
{
    #[inline]
    fn drop(&mut self) {
        // drop all events that were not consumed
        self.events.clear();

        // the slot can use the buffer again
        self.slot.recycle(std::mem::take(&mut self.events));
    }
}
