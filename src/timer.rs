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
#[derive(Debug)]
pub struct Timer {
    duration: Duration,
    state: Rc<State>,
}

struct State {
    secs_left_changed_cb: Box<dyn Fn(u64) + 'static>,
    secs_left_changed_source_id: RefCell<Option<glib::SourceId>>,

    is_done: Cell<bool>,
    is_cancelled: Cell<bool>,

    instant: Cell<Option<Instant>>,
    waker: RefCell<Option<Waker>>,
    source_id: RefCell<Option<glib::SourceId>>,
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("is_done", &self.is_done)
            .field("is_cancelled", &self.is_cancelled)
            .field("elapsed", &self.instant.get().map(|i| i.elapsed()))
            .finish()
    }
}

impl Timer {
    /// The timer will start as it gets polled
    pub fn new(duration: Duration, secs_left_changed_cb: impl Fn(u64) + 'static) -> Self {
        Self {
            duration,
            state: Rc::new(State {
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
        self.state.is_cancelled.set(true);

        if let Some(source_id) = self.state.source_id.take() {
            source_id.remove();
        }

        if let Some(source_id) = self.state.secs_left_changed_source_id.take() {
            source_id.remove();
        }

        if let Some(waker) = self.state.waker.take() {
            waker.wake();
        }
    }
}

impl Clone for Timer {
    fn clone(&self) -> Self {
        Self {
            duration: self.duration,
            state: Rc::clone(&self.state),
        }
    }
}

impl Future for Timer {
    type Output = Result;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.duration == Duration::ZERO {
            self.state.is_done.set(true);
            return Poll::Ready(Result::Ok);
        }

        let waker = cx.waker().clone();
        self.state.waker.replace(Some(waker));

        let duration = self.duration;
        self.state
            .secs_left_changed_source_id
            .replace(Some(glib::timeout_add_local(
                Duration::from_millis(200),
                clone!(@weak self.state as state => @default-return Continue(false), move || {
                    let elapsed_secs = state
                        .instant
                        .get()
                        .map_or(Duration::ZERO, |instant| instant.elapsed())
                        .as_secs();

                    let secs_left = duration.as_secs() - elapsed_secs;
                    (state.secs_left_changed_cb)(secs_left);
                    Continue(true)
                }),
            )));

        self.state
            .source_id
            .replace(Some(glib::timeout_add_local_once(
                self.duration,
                clone!(@weak self.state as state => move || {
                    state.is_done.set(true);

                    if let Some(source_id) = state.secs_left_changed_source_id.take() {
                        source_id.remove();
                    }

                    if let Some(waker) = state.waker.take() {
                        waker.wake();
                    }
                }),
            )));
        self.state.instant.set(Some(Instant::now()));
        (self.state.secs_left_changed_cb)(duration.as_secs());

        if self.state.is_cancelled.get() {
            Poll::Ready(Result::Cancelled)
        } else if self.state.is_done.get() {
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
