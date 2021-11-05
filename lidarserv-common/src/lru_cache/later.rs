use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Later error type
#[derive(Error, Debug)]
pub enum LaterError {
    /// Error that is returned by [Later::get] and related methods, if the sender was dropped
    /// without ever calling [LaterSender::notify].
    #[error("sender disconnected")]
    Disconnected,
}

/// Sends a value, that others can wait for.
pub struct LaterSender<T> {
    inner: Arc<Mutex<LaterInner<T>>>,
}

/// Allows to wait for a value being sent by [LaterSender]
#[derive(Clone)]
pub enum Later<T> {
    Later(Arc<Mutex<LaterInner<T>>>),
    Exists(T),
}

pub enum LaterInner<T> {
    Later(Vec<crossbeam_channel::Sender<T>>),
    Available(T),
}

impl<T> LaterSender<T> {
    /// Creates a new sender
    pub fn new() -> Self {
        let inner = LaterInner::<T>::Later(Vec::new());
        let inner = Mutex::new(inner);
        let inner = Arc::new(inner);
        LaterSender { inner }
    }

    /// Makes all [Later] instances tied to this sender resolve with the given value.
    /// Any currently blocking calls to [Later::get] or [Later::into] will wake up and return the
    /// provided value, any future calls will return immediately.
    pub fn send(self, value: T)
    where
        T: Clone,
    {
        // acquire mutex
        let mut lock = self.inner.lock().unwrap();

        // notify
        let waiting = match &*lock {
            LaterInner::Later(w) => w,
            LaterInner::Available(_) => {
                unreachable!()
            }
        };
        for sender in waiting {
            let value = value.clone();
            sender.send(value).ok();
        }

        // set inner value
        *lock = LaterInner::Available(value);
    }

    /// Returns a [Later] value, that is tied to this sender.
    pub fn later(&self) -> Later<T> {
        let inner = Arc::clone(&self.inner);
        Later::Later(inner)
    }
}

impl<T> Default for LaterSender<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Later<T>
where
    T: Clone,
{
    /// Creates a new instance, that is immediately resolved with the given value.
    pub fn new(value: T) -> Self {
        Later::Exists(value)
    }

    /// Returns the value passed to [LaterSender::send] of the corresponding sender.
    /// If it has not been called yet, it will block until [LaterSender::send] is called.
    pub fn into(self) -> Result<T, LaterError> {
        match self {
            Later::Exists(value) => Ok(value),
            Later::Later(inner) => Self::wait_get(&inner),
        }
    }

    fn wait_get(inner: &Mutex<LaterInner<T>>) -> Result<T, LaterError> {
        let mut lock = inner.lock().unwrap();

        match &mut *lock {
            LaterInner::Available(value) => Ok(value.clone()),
            LaterInner::Later(waiting) => {
                // add to list of waiters
                let (sender, receiver) = crossbeam_channel::unbounded();
                waiting.push(sender);

                // release lock
                drop(lock);

                // wait for the value to be sent to us
                receiver.recv().map_err(|_| LaterError::Disconnected)
            }
        }
    }
}
