use std::{collections::VecDeque, marker::PhantomData, sync::MutexGuard};

use crate::backend::Event;

#[derive(Debug)]
/// An iterator over events from type `T`.
pub struct Query<'a, T, const EVENT_SIZE: usize>
where
    T: 'static,
{
    events: MutexGuard<'a, VecDeque<Event<EVENT_SIZE>>>,

    _t: PhantomData<T>,
}

impl<'a, T, const EVENT_SIZE: usize> Query<'a, T, EVENT_SIZE>
where
    T: 'static,
{
    /// Creates a new `Query` to iterate over events from type `T`.
    #[inline]
    pub(crate) fn new(events: MutexGuard<'a, VecDeque<Event<EVENT_SIZE>>>) -> Self {
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

impl<'a, T, const EVENT_SIZE: usize> Iterator for Query<'a, T, EVENT_SIZE>
where
    T: 'static,
{
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let out = self.events.pop_front();

        out.map(Event::get)
    }
}

impl<'a, T, const EVENT_SIZE: usize> Drop for Query<'a, T, EVENT_SIZE>
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
pub struct UnblockingQuery<T, const EVENT_SIZE: usize>
where
    T: 'static,
{
    events: VecDeque<Event<EVENT_SIZE>>,

    _t: PhantomData<T>,
}

impl<T, const EVENT_SIZE: usize> UnblockingQuery<T, EVENT_SIZE>
where
    T: 'static,
{
    #[inline]
    /// Creates a new `Query` to iterate over events from type `T`.
    pub(crate) fn new(events: VecDeque<Event<EVENT_SIZE>>) -> Self {
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

impl<T, const EVENT_SIZE: usize> Iterator for UnblockingQuery<T, EVENT_SIZE>
where
    T: 'static,
{
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let out = self.events.pop_front();

        out.map(Event::get)
    }
}

impl<T, const EVENT_SIZE: usize> Drop for UnblockingQuery<T, EVENT_SIZE>
where
    T: 'static,
{
    #[inline]
    fn drop(&mut self) {
        self.events.clear();
    }
}
