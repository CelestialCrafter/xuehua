use std::marker::PhantomData;

use std::error::Error as StdError;
use thiserror::Error;

#[derive(Error, Debug)]
struct ContextError<O, W> {
    original: O,
    wrapper: W,
}

#[derive(Error)]
pub struct Report<E> {
    #[source]
    inner: &'static (dyn StdError + Send + Sync + 'static),
    hints: Vec<String>,
    _original: PhantomData<E>,
}

impl<E> Report<E> {
    fn hint(mut self, msg: String) -> Self {
        self.hints.push(msg);
        self
    }

    fn context<W>(mut self, wrapper: W) -> Self {
        self.inner = ContextError {
            original: self.inner,
            wrapper,
        } as &dyn StdError;

        self
    }
}
