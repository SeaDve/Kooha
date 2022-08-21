use anyhow::{bail, Result};
use futures_util::{
    future::{self, Either, Future},
    pin_mut,
};
use gtk::gio::glib;

use std::{path::Path, time::Duration};

/// Spawns a future in the default [`glib::MainContext`]
pub fn spawn<F: std::future::Future<Output = ()> + 'static>(fut: F) {
    let ctx = glib::MainContext::default();
    ctx.spawn_local(fut);
}

/// Whether the application is running in a flatpak sandbox.
pub fn is_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

pub async fn future_timeout<T>(fut: impl Future<Output = T>, timeout: Duration) -> Result<T> {
    pin_mut!(fut);

    match future::select(fut, glib::timeout_future(timeout)).await {
        Either::Left((res, _)) => Ok(res),
        Either::Right(_) => bail!("Operation timeout"),
    }
}
