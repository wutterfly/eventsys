use std::marker::PhantomData;

pub struct Value;

pub struct NoValue;

pub struct EventError<T: 'static, V = NoValue> {
    inner: Option<T>,
    raw: RawErr<T>,
    v: PhantomData<V>,
}

impl<T: 'static, V> EventError<T, V> {
    pub const fn raw_err(&self) -> RawErr<T> {
        match self.raw {
            RawErr::UnregisteredEventType(_) => RawErr::UnregisteredEventType(PhantomData),
            RawErr::RegisteredWithoutStore => RawErr::RegisteredWithoutStore,
        }
    }
}

impl<T: 'static> EventError<T, Value> {
    pub fn into_inner(self) -> T {
        debug_assert!(self.inner.is_some());
        // SAFETY:
        // Unwrapping this value is safe, because it is guaranteed with the marker generic Value,
        // that this Option contains a value.
        unsafe { self.inner.unwrap_unchecked() }
    }

    pub const fn unregistered_event(value: T) -> Self {
        Self {
            inner: Some(value),
            v: PhantomData,
            raw: RawErr::UnregisteredEventType(PhantomData),
        }
    }
}

impl<T: 'static> EventError<T, NoValue> {
    pub const fn unregisted_event_empty() -> Self {
        Self {
            inner: None,
            raw: RawErr::UnregisteredEventType(PhantomData),
            v: PhantomData,
        }
    }

    pub const fn registered_without_store() -> Self {
        Self {
            inner: None,
            raw: RawErr::RegisteredWithoutStore,
            v: PhantomData,
        }
    }
}

impl<T: 'static, V> std::error::Error for EventError<T, V> {}

impl<T: 'static, V> std::fmt::Debug for EventError<T, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoxedEventError")
            .field("inner", &if self.inner.is_some() { "Some" } else { "None" })
            .field("raw", &self.raw)
            .finish()
    }
}

impl<T: 'static, V> std::fmt::Display for EventError<T, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.raw)
    }
}

#[derive(Clone, Copy)]
pub enum RawErr<T: 'static> {
    UnregisteredEventType(PhantomData<T>),
    RegisteredWithoutStore,
}

impl<T: 'static> std::error::Error for RawErr<T> {}

impl<T: 'static> std::fmt::Debug for RawErr<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = std::any::type_name::<T>();
        match self {
            Self::UnregisteredEventType(_) => {
                f.debug_tuple("UnregisteredEventType").field(&name).finish()
            }

            Self::RegisteredWithoutStore => write!(f, "RegisteredWithListener"),
        }
    }
}

impl<T: 'static> std::fmt::Display for RawErr<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = std::any::type_name::<T>();

        match self {
            Self::UnregisteredEventType(_) => {
                write!(f, "Unregistered event type: {name}")
            }

            Self::RegisteredWithoutStore => {
                write!(f, "Event type was not registered to store events")
            }
        }
    }
}
