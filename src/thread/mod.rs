use core::fmt;
pub mod pool;

pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T,
    F: Send + 'static,
    T: Send + 'static,
{
    Builder::new().spawn(f).expect("failed to spawn thread")
}

pub struct Builder {
    inner: jod_thread::Builder,
    allow_leak: bool,
}

impl Builder {
    pub fn new() -> Self {
        Builder {
            inner: jod_thread::Builder::new(),
            allow_leak: false,
        }
    }

    pub fn name(self, name: String) -> Builder {
        Builder {
            inner: self.inner.name(name),
            ..self
        }
    }

    pub fn stack_size(self, size: usize) -> Builder {
        Builder {
            inner: self.inner.stack_size(size),
            ..self
        }
    }

    pub fn allow_leak(self, b: bool) -> Builder {
        Builder {
            allow_leak: b,
            ..self
        }
    }

    pub fn spawn<F, T>(self, f: F) -> std::io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T,
        F: Send + 'static,
        T: Send + 'static,
    {
        let inner_handle = self.inner.spawn(move || {
            f()
        })?;

        Ok(JoinHandle { inner: Some(inner_handle), allow_leak: self.allow_leak })
    }
}

pub struct JoinHandle<T = ()> {
    // `inner` is an `Option` so that we can
    // take ownership of the contained `JoinHandle`.
    inner: Option<jod_thread::JoinHandle<T>>,
    allow_leak: bool,
}

impl<T> JoinHandle<T> {
    pub fn join(mut self) -> T {
        self.inner.take().unwrap().join()
    }
}

impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        if !self.allow_leak {
            return;
        }

        if let Some(join_handle) = self.inner.take() {
            join_handle.detach();
        }
    }
}

impl<T> fmt::Debug for JoinHandle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("JoinHandle { .. }")
    }
}