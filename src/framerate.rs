use gtk::{gio, glib::BoxedAnyObject};

use crate::settings::Settings;

static BUILTINS: &[gst::Fraction] = &[
    gst::Fraction::from_integer(10),
    gst::Fraction::from_integer(20),
    gst::Fraction::from_integer(24),
    gst::Fraction::from_integer(25),
    gst::Fraction::new_raw(30_000, 1001), // 29.97
    gst::Fraction::from_integer(30),
    gst::Fraction::from_integer(48),
    gst::Fraction::from_integer(50),
    gst::Fraction::new_raw(60_000, 1001), // 59.94
    gst::Fraction::from_integer(60),
];

/// Returns a model of type `BoxedAnyObject`.
///
/// This appends the current framerate in the settings if it does not match any built-in ones.
pub fn builtins_model(settings: &Settings) -> gio::ListStore {
    let list_store = gio::ListStore::new::<BoxedAnyObject>();

    let items = BUILTINS
        .iter()
        .map(|fraction| BoxedAnyObject::new(*fraction))
        .collect::<Vec<_>>();
    list_store.splice(0, 0, &items);

    let other = settings.framerate();
    if !BUILTINS.contains(&other) {
        list_store.append(&BoxedAnyObject::new(other));
    }

    list_store
}

/// Formats a framerate in a human-readable format.
pub fn format(framerate: gst::Fraction) -> String {
    let reduced = framerate.reduced();

    if reduced.is_integer() {
        return reduced.numer().to_string();
    }

    let float = *reduced.numer() as f64 / *reduced.denom() as f64;
    format!("{:.2}", float)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn assert_simplified(framerate: gst::Fraction) {
        let reduced = framerate.0.reduced();

        assert_eq!(
            (framerate.numer(), framerate.denom()),
            (*reduced.numer(), *reduced.denom())
        );
    }

    #[test]
    fn simplified() {
        for framerate in BUILTINS {
            assert_simplified(*framerate);
        }
    }

    #[test]
    fn test_format() {
        assert_eq!(format(gst::Fraction::from_integer(24)), "24");
        assert_eq!(format(gst::Fraction::new(30_000, 1001)), "29.97");
        assert_eq!(format(gst::Fraction::new(60_000, 1001)), "59.94");
    }
}
