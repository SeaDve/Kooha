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

#[derive(Debug, Clone, Copy)]
#[must_use]
pub enum Result {
    Ok,
    Cancelled,
}

impl Result {
    pub fn is_cancelled(self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

/// Reference counted cancellable timer future
#[derive(Clone)]
pub struct Timer {
    inner: Rc<Inner>,
}

impl fmt::Debug for Timer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("duration", &self.inner.duration)
            .field("is_done", &self.inner.is_done.get())
            .field("is_cancelled", &self.inner.is_cancelled.get())
            .field("elapsed", &self.inner.instant.get().map(|i| i.elapsed()))
            .finish()
    }
}

struct Inner {
    duration: Duration,

    secs_left_changed_cb: Box<dyn Fn(u64) + 'static>,
    secs_left_changed_source_id: RefCell<Option<glib::SourceId>>,

    is_done: Cell<bool>,
    is_cancelled: Cell<bool>,

    instant: Cell<Option<Instant>>,
    waker: RefCell<Option<Waker>>,
    source_id: RefCell<Option<glib::SourceId>>,
}

impl Inner {
    fn secs_left(&self) -> u64 {
        if self.is_done.get() {
            return 0;
        }

        if self.is_cancelled.get() {
            return 0;
        }

        let elapsed_secs = self
            .instant
            .get()
            .map_or(Duration::ZERO, |instant| instant.elapsed())
            .as_secs();

        self.duration.as_secs() - elapsed_secs
    }
}

impl Timer {
    /// The timer will start as it gets polled
    pub fn new(duration: Duration, secs_left_changed_cb: impl Fn(u64) + 'static) -> Self {
        Self {
            inner: Rc::new(Inner {
                duration,
                secs_left_changed_cb: Box::new(secs_left_changed_cb),
                secs_left_changed_source_id: RefCell::new(None),
                is_done: Cell::new(false),
                is_cancelled: Cell::new(false),
                instant: Cell::new(None),
                waker: RefCell::new(None),
                source_id: RefCell::new(None),
            }),
        }
    }

    pub fn cancel(&self) {
        self.inner.is_cancelled.set(true);

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
    type Output = Result;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.inner.duration == Duration::ZERO {
            self.inner.is_done.set(true);
            return Poll::Ready(Result::Ok);
        }

        let waker = cx.waker().clone();
        self.inner.waker.replace(Some(waker));

        self.inner
            .secs_left_changed_source_id
            .replace(Some(glib::timeout_add_local(
                Duration::from_millis(200),
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
                    inner.is_done.set(true);

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

        if self.inner.is_cancelled.get() {
            Poll::Ready(Result::Cancelled)
        } else if self.inner.is_done.get() {
            Poll::Ready(Result::Ok)
        } else {
            Poll::Pending
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.cancel();
    }
}
