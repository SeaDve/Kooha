use gtk::glib;

/// Spawns a future in the default [`glib::MainContext`]
pub fn spawn<F: std::future::Future<Output = ()> + 'static>(fut: F) {
    let ctx = glib::MainContext::default();
    ctx.spawn_local(fut);
}
