use gtk::glib;

use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub struct HandleToken(String);

impl HandleToken {
    pub fn new() -> Self {
        Self(format!("kooha_{}", COUNTER.fetch_add(1, Ordering::Relaxed)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl glib::ToVariant for HandleToken {
    fn to_variant(&self) -> glib::Variant {
        self.0.to_variant()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniqueness() {
        let a = HandleToken::new();
        let b = HandleToken::new();
        let c = HandleToken::new();

        assert_ne!(a.as_str(), b.as_str());
        assert_ne!(b.as_str(), c.as_str());
        assert_ne!(a.as_str(), c.as_str());
    }
}
