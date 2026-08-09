use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
    thread,
};

pub(crate) fn dispatch<T, Work>(work: Work) -> Completion<T>
where
    T: Send + 'static,
    Work: FnOnce(CompletionSender<T>) + Send + 'static,
{
    let state = Arc::new(Mutex::new(CompletionState {
        value: None,
        waker: None,
    }));
    let sender = CompletionSender {
        state: Arc::clone(&state),
    };
    thread::spawn(move || work(sender));
    Completion { state }
}

struct CompletionState<T> {
    value: Option<T>,
    waker: Option<Waker>,
}

pub(crate) struct CompletionSender<T> {
    state: Arc<Mutex<CompletionState<T>>>,
}

impl<T> CompletionSender<T> {
    pub(crate) fn send(self, value: T) {
        let waker = self.store(value);
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    pub(crate) fn store(self, value: T) -> Option<Waker> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.value = Some(value);
        state.waker.take()
    }
}

pub(crate) struct Completion<T> {
    state: Arc<Mutex<CompletionState<T>>>,
}

impl<T> Future for Completion<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(value) = state.value.take() {
            Poll::Ready(value)
        } else {
            state.waker = Some(context.waker().clone());
            Poll::Pending
        }
    }
}
