use futures_util::future::FusedFuture;
use gtk::glib::{self, clone, Continue};

use std::{
    cell::{Cell, RefCell},
    fmt,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

use crate::cancelled::Cancelled;

const DEFAULT_SECS_LEFT_UPDATE_INTERVAL: Duration = Duration::from_millis(200);

/// Reference counted cancellable timer future
#[derive(Clone)]
pub struct Timer {
    inner: Rc<Inner>,
}

impl fmt::Debug for Timer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Timer")
            .field("duration", &self.inner.duration)
            .field("state", &self.inner.state.get())
            .field("elapsed", &self.inner.instant.get().map(|i| i.elapsed()))
            .finish()
    }
}

#[derive(Debug, Clone, Copy)]
enum State {
    Waiting,
    Cancelled,
    Done,
}

impl State {
    fn to_poll(self) -> Poll<<Timer as Future>::Output> {
        match self {
            State::Waiting => Poll::Pending,
            State::Cancelled => Poll::Ready(Err(Cancelled::new("timer"))),
            State::Done => Poll::Ready(Ok(())),
        }
    }
}

struct Inner {
    duration: Duration,

    secs_left_changed_cb: Box<dyn Fn(u64) + 'static>,
    secs_left_changed_source_id: RefCell<Option<glib::SourceId>>,

    state: Cell<State>,

    instant: Cell<Option<Instant>>,
    waker: RefCell<Option<Waker>>,
    source_id: RefCell<Option<glib::SourceId>>,
}

impl Inner {
    fn secs_left(&self) -> u64 {
        if self.is_terminated() {
            return 0;
        }

        let elapsed_secs = self
            .instant
            .get()
            .map_or(Duration::ZERO, |instant| instant.elapsed())
            .as_secs();

        self.duration.as_secs() - elapsed_secs
    }

    fn is_terminated(&self) -> bool {
        matches!(self.state.get(), State::Done | State::Cancelled)
    }
}

impl Timer {
    /// The timer will start as soon as it gets polled
    pub fn new(duration: Duration, secs_left_changed_cb: impl Fn(u64) + 'static) -> Self {
        Self {
            inner: Rc::new(Inner {
                duration,
                secs_left_changed_cb: Box::new(secs_left_changed_cb),
                secs_left_changed_source_id: RefCell::new(None),
                state: Cell::new(State::Waiting),
                instant: Cell::new(None),
                waker: RefCell::new(None),
                source_id: RefCell::new(None),
            }),
        }
    }

    pub fn cancel(&self) {
        if self.inner.is_terminated() {
            return;
        }

        self.inner.state.set(State::Cancelled);

        if let Some(source_id) = self.inner.source_id.take() {
            source_id.remove();
        }

        if let Some(source_id) = self.inner.secs_left_changed_source_id.take() {
            source_id.remove();
        }

        if let Some(waker) = self.inner.waker.take() {
            waker.wake();
        }
    }
}

impl Future for Timer {
    type Output = Result<(), Cancelled>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.inner.state.get().to_poll() {
            ready @ Poll::Ready(_) => return ready,
            Poll::Pending => {}
        }

        if self.inner.duration == Duration::ZERO {
            self.inner.state.set(State::Done);
            return Poll::Ready(Ok(()));
        }

        let waker = cx.waker().clone();
        self.inner.waker.replace(Some(waker));

        self.inner
            .secs_left_changed_source_id
            .replace(Some(glib::timeout_add_local(
                DEFAULT_SECS_LEFT_UPDATE_INTERVAL,
                clone!(@weak self.inner as inner => @default-return Continue(false), move || {
                    (inner.secs_left_changed_cb)(inner.secs_left());
                    Continue(true)
                }),
            )));

        self.inner
            .source_id
            .replace(Some(glib::timeout_add_local_once(
                self.inner.duration,
                clone!(@weak self.inner as inner => move || {
                    inner.state.set(State::Done);

                    if let Some(source_id) = inner.secs_left_changed_source_id.take() {
                        source_id.remove();
                    }

                    if let Some(waker) = inner.waker.take() {
                        waker.wake();
                    }
                }),
            )));
        self.inner.instant.set(Some(Instant::now()));
        (self.inner.secs_left_changed_cb)(self.inner.secs_left());

        self.inner.state.get().to_poll()
    }
}

impl FusedFuture for Timer {
    fn is_terminated(&self) -> bool {
        self.inner.is_terminated()
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures_util::FutureExt;

    #[gtk::test]
    async fn normal() {
        let timer = Timer::new(Duration::from_nanos(10), |_| {});
        assert_eq!(timer.inner.duration, Duration::from_nanos(10));
        assert!(matches!(timer.inner.state.get(), State::Waiting));

        assert!(timer.clone().await.is_ok());
        assert!(matches!(timer.inner.state.get(), State::Done));
        assert_eq!(timer.inner.secs_left(), 0);
    }

    #[gtk::test]
    async fn cancelled() {
        let timer = Timer::new(Duration::from_nanos(10), |_| {});
        assert!(matches!(timer.inner.state.get(), State::Waiting));

        timer.cancel();

        assert!(timer.clone().await.is_err());
        assert!(matches!(timer.inner.state.get(), State::Cancelled));
        assert_eq!(timer.inner.secs_left(), 0);
    }

    #[gtk::test]
    fn zero_duration() {
        let control = Timer::new(Duration::from_nanos(10), |_| {});
        assert!(control.now_or_never().is_none());

        let timer = Timer::new(Duration::ZERO, |_| {});

        assert!(timer.clone().now_or_never().unwrap().is_ok());
        assert!(matches!(timer.inner.state.get(), State::Done));
        assert_eq!(timer.inner.secs_left(), 0);
    }
}
