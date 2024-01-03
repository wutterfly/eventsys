use std::{
    any::TypeId,
    collections::VecDeque,
    panic::RefUnwindSafe,
    sync::{atomic::AtomicBool, MutexGuard},
};

pub type Event<const SIZE: usize> = anythingy::Thing<SIZE>;

use crate::{
    err::{EventError, EventSizeError, Value},
    map::RegisteredMap,
    query::{Query, UnblockingQuery},
    slot::{Slot, SlotType},
    DEFAULT_EVENT_SIZE,
};

/// System to register events and event listeners as well as dispatch and query events.
///
///
/// Events can be either handled with an event listener or be registered and then stored and handled in batches later.
///
/// Events can be any type. For efficient event dispatching, all event types have to be the same size.
/// For very big events or Dynamically Sized Types (DSTs) events can be boxed.
pub struct EventBackend<const EVENT_SIZE: usize = DEFAULT_EVENT_SIZE> {
    pub(crate) registered: RegisteredMap<EVENT_SIZE>,
}

impl<const EVENT_SIZE: usize> EventBackend<EVENT_SIZE> {
    #[must_use]
    /// Creates a new `EventBackend`.
    pub const fn new() -> Self {
        Self {
            registered: RegisteredMap::new(),
        }
    }

    /// Registers a new type of event. Registered events can be querried in a batch.
    ///
    /// # Errors
    /// Returns an `EventError`, if
    ///     - the type can not be used as an event
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// system.register_store::<u32>(SlotType::All).unwrap();
    /// system.register_store::<(u16, u16)>(SlotType::Last).unwrap();
    /// # }
    /// ```
    pub fn register_store<T: 'static>(&mut self, typ: SlotType<T>) -> Result<(), EventError<T>> {
        // check if T can be used as an event
        if !Event::<EVENT_SIZE>::fitting::<T>() {
            return Err(EventError::event_size_empty(EventSizeError::new(
                EVENT_SIZE,
                Event::<EVENT_SIZE>::size_requirement::<T>(),
            )));
        }

        let id = TypeId::of::<T>();

        let slot = Slot::new(typ);

        if let Some(registered) = self.registered.get_mut(&id) {
            registered.slot = Some(slot);
            return Ok(());
        }

        let mut registered = Registered::new();
        registered.slot = Some(slot);
        _ = self.registered.insert(id, registered);

        Ok(())
    }

    /// Registers a function that gets called, if an event with the matching type is triggered.
    /// Returns the number of listener registered for this type of event.
    ///
    /// # Errors
    /// Returns an `EventError`, if
    ///     - the type can not be used as an event
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::EventBackend;
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// let listener = |event: &u32| {
    ///     // handle event
    /// };
    ///
    /// system.register_listener::<u32>(listener).unwrap();
    /// # }
    /// ```
    pub fn register_listener<T: 'static>(
        &mut self,
        listener: impl Fn(&T) + Send + Sync + RefUnwindSafe + 'static,
    ) -> Result<usize, EventSizeError> {
        // check if T can be used as an event
        if !Event::<EVENT_SIZE>::fitting::<T>() {
            return Err(EventSizeError::new(
                EVENT_SIZE,
                Event::<EVENT_SIZE>::size_requirement::<T>(),
            ));
        }

        let id = TypeId::of::<T>();

        let map_f = move |event: &Event<EVENT_SIZE>| {
            let value = event.get_ref::<T>();
            listener(value);
        };

        if let Some(registered) = self.registered.get_mut(&id) {
            registered.listener.push(Box::new(map_f));
            return Ok(registered.listener.len());
        }

        let mut registered = Registered::new();
        registered.listener.push(Box::new(map_f));
        _ = self.registered.insert(id, registered);

        Ok(1)
    }

    /// Triggers a new event, calling all registered event listener. If event was registered to be stored,
    /// event gets saved to be queried later after each listener was called.
    ///
    /// # Errors
    /// Returns an `EventError`, if
    ///     - the type can not be used as an event
    ///     - the event is not registered for storage and no event listener was set
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// system.register_store::<u32>(SlotType::First).unwrap();
    ///
    /// system.new_event::<u32>(42).unwrap();
    /// # }
    /// ```
    pub fn new_event<T: 'static>(&self, value: T) -> Result<(), EventError<T, Value>> {
        // check if T can be used as an event
        if !Event::<EVENT_SIZE>::fitting::<T>() {
            let err = EventSizeError::new(EVENT_SIZE, Event::<EVENT_SIZE>::size_requirement::<T>());
            return Err(EventError::event_size(value, err));
        }

        let id = TypeId::of::<T>();

        if let Some(registered) = self.registered.get(&id) {
            registered.handle_event(Event::new(value));
        } else {
            return Err(EventError::unregisted_event(value));
        }

        Ok(())
    }

    /// Returns an iterator over each event with the matching event type.
    ///
    /// # Errors
    /// Returns an `UnregisteredEventType` error, if the given type was not registered as event type.
    /// Returns an `RegisteredWithoutStore` error, if the queried type is not registered to store events.
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// # system.register_store::<u32>(SlotType::All);
    /// let query = system.query::<u32>().unwrap();
    ///
    /// for event in query {
    ///     // handle event
    /// }
    /// # }
    /// ```
    pub fn query<T: 'static>(&self) -> Result<UnblockingQuery<T, EVENT_SIZE>, EventError<T>> {
        // check if T can be used as an event
        if !Event::<EVENT_SIZE>::fitting::<T>() {
            return Err(EventError::event_size_empty(EventSizeError::new(
                EVENT_SIZE,
                Event::<EVENT_SIZE>::size_requirement::<T>(),
            )));
        }

        let id = TypeId::of::<T>();

        self.registered.get(&id).map_or_else(
            || Err(EventError::unregisted_event_empty()),
            |registed| {
                registed.events_clone().map_or_else(
                    || Err(EventError::registered_without_store()),
                    |events| Ok(UnblockingQuery::new(events)),
                )
            },
        )
    }

    /// Returns an iterator over each event with the matching event type.
    ///
    /// # Warning
    /// Holding the query will block access to this event type, but will not clone the underlying data. For not-blocking but cloning query, see [`EventBackend::query`].
    ///
    /// # Errors
    /// Returns an `UnregisteredEventType` error, if the given type was not registered as event type.
    /// Returns an `RegisteredWithoutStore` error, if the queried type is not registered to store events.
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// # system.register_store::<u32>(SlotType::All);
    /// let query = system.query_blocking::<u32>().unwrap();
    ///
    /// for event in query {
    ///     // handle event
    /// }
    /// # }
    /// ```
    pub fn query_blocking<T: 'static>(&self) -> Result<Query<T, EVENT_SIZE>, EventError<T>> {
        // check if T can be used as an event
        if !Event::<EVENT_SIZE>::fitting::<T>() {
            return Err(EventError::event_size_empty(EventSizeError::new(
                EVENT_SIZE,
                Event::<EVENT_SIZE>::size_requirement::<T>(),
            )));
        }

        let id = TypeId::of::<T>();

        self.registered.get(&id).map_or_else(
            || Err(EventError::unregisted_event_empty()),
            |registed| {
                registed.events().map_or_else(
                    || Err(EventError::registered_without_store()),
                    |events| Ok(Query::new(events)),
                )
            },
        )
    }

    /// Disables specific event from being processed.
    ///
    /// # Errors
    /// Returns an `UnregisteredEventTypeError`, if the event type is not registered or no event listener was registered.
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// # system.register_store::<u64>(SlotType::All);
    /// system.disable::<u64>().unwrap();
    /// # }
    /// ```
    pub fn disable<T: 'static>(&self) -> Result<(), EventError<T>> {
        // check if T can be used as an event
        if !Event::<EVENT_SIZE>::fitting::<T>() {
            return Err(EventError::event_size_empty(EventSizeError::new(
                EVENT_SIZE,
                Event::<EVENT_SIZE>::size_requirement::<T>(),
            )));
        }

        let id = TypeId::of::<T>();

        match self.registered.get(&id) {
            Some(registered) => registered.disable(),
            None => return Err(EventError::unregisted_event_empty()),
        }

        Ok(())
    }

    /// Disables all events.
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// system.disable_all();
    /// # }
    /// ```
    pub fn disable_all(&self) {
        for registered in self.registered.values() {
            registered.disable();
        }
    }

    /// Enables specific event for processesing.
    ///
    /// # Errors
    /// Returns an `UnregisteredEventTypeError`, if the event type is not registered or no event listener was registered.
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// # system.register_store::<u64>(SlotType::Last);
    /// system.enable::<u64>().unwrap();
    /// # }
    /// ```
    pub fn enable<T: 'static>(&self) -> Result<(), EventError<T>> {
        // check if T can be used as an event
        if !Event::<EVENT_SIZE>::fitting::<T>() {
            return Err(EventError::event_size_empty(EventSizeError::new(
                EVENT_SIZE,
                Event::<EVENT_SIZE>::size_requirement::<T>(),
            )));
        }

        let id = TypeId::of::<T>();

        match self.registered.get(&id) {
            Some(registered) => registered.enable(),
            None => return Err(EventError::unregisted_event_empty()),
        }

        Ok(())
    }

    /// Enables all events.
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// system.enable_all();
    /// # }
    /// ```
    pub fn enable_all(&self) {
        for registered in self.registered.values() {
            registered.enable();
        }
    }

    /// Frees allocated memory for batch events.
    ///
    /// # Warn
    /// All events that are not consumed will get dropped.
    pub fn cleanup(&mut self) {
        for registered in self.registered.values_mut() {
            registered.cleanup();
        }
    }
}

impl Default for EventBackend<DEFAULT_EVENT_SIZE> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<const EVENT_SIZE: usize> std::fmt::Debug for EventBackend<EVENT_SIZE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBackend")
            .field("registered", &self.registered.len())
            .finish()
    }
}

type Listener<const SIZE: usize> = Box<dyn Fn(&Event<SIZE>) + Sync + RefUnwindSafe + Send>;

pub struct Registered<const SIZE: usize> {
    slot: Option<Slot<SIZE>>,
    listener: Vec<Listener<SIZE>>,
    enabled: AtomicBool,
}

impl<const SIZE: usize> Registered<SIZE> {
    #[inline]
    pub fn new() -> Self {
        Self {
            slot: None,
            listener: Vec::new(),
            enabled: AtomicBool::new(true),
        }
    }

    pub fn handle_event(&self, event: Event<SIZE>) {
        // check if events for this registered type should be processed
        if !self.enabled.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }

        // call all listeners
        for listener in &self.listener {
            _ = std::panic::catch_unwind(|| (listener)(&event));
        }

        // store event for querying it later
        if let Some(slot) = &self.slot {
            slot.push(event);
        }
    }

    #[inline]
    pub fn events_clone(&self) -> Option<VecDeque<Event<SIZE>>> {
        self.slot.as_ref().map(Slot::events_clone)
    }

    #[inline]
    pub fn events(&self) -> Option<MutexGuard<VecDeque<Event<SIZE>>>> {
        self.slot.as_ref().map(Slot::events)
    }

    #[inline]
    pub fn cleanup(&mut self) {
        self.listener = Vec::new();

        if let Some(slot) = &mut self.slot {
            slot.cleanup();
        }
    }

    #[inline]
    fn enable(&self) {
        self.enabled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    #[inline]
    fn disable(&self) {
        self.enabled
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

impl<const SIZE: usize> std::fmt::Debug for Registered<SIZE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registered")
            .field("slot", &self.slot)
            .field("listener", &self.listener.len())
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::DEFAULT_EVENT_SIZE;

    use super::EventBackend;

    const fn const_listener<E>(_: &E) {}

    #[test]
    fn test_eventbackend_setup_listener() {
        let mut events: EventBackend<DEFAULT_EVENT_SIZE> = EventBackend::new();

        let res = events.register_listener(const_listener::<u32>);

        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 1);

        let res = events.register_listener(const_listener::<u32>);

        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 2);
    }

    #[test]
    fn test_eventbackend_setup_register() {
        let mut events: EventBackend<DEFAULT_EVENT_SIZE> = EventBackend::new();

        let res = events.register_listener(const_listener::<u32>);

        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 1);

        let res = events.register_listener(const_listener::<u32>);

        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 2);
    }

    #[test]
    fn test_eventbackend_setup_mixed() {
        let mut events: EventBackend<DEFAULT_EVENT_SIZE> = EventBackend::new();

        let res = events.register_listener::<u32>(const_listener);

        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 1);

        let res = events.register_store::<u32>(crate::SlotType::First);

        assert!(res.is_ok());
    }
}
