use std::fmt;

use gtk::{gio, glib::BoxedAnyObject};
use num_traits::Signed;

use crate::{pipeline::Framerate, settings::Settings};

/// The available options for the framerate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramerateOption {
    _10,
    _20,
    _24,
    _25,
    _29_97,
    _30,
    _48,
    _50,
    _59_94,
    _60,
    Other(Framerate),
}

impl FramerateOption {
    fn all_except_other() -> [Self; 10] {
        [
            Self::_10,
            Self::_20,
            Self::_24,
            Self::_25,
            Self::_29_97,
            Self::_30,
            Self::_48,
            Self::_50,
            Self::_59_94,
            Self::_60,
        ]
    }

    /// Returns a model of type `BoxedAnyObject`. This contains `Other` if the current settings framerate
    /// does not match any of the predefined options.
    pub fn model(settings: &Settings) -> gio::ListStore {
        let list_store = gio::ListStore::new::<BoxedAnyObject>();

        let items = Self::all_except_other()
            .into_iter()
            .map(BoxedAnyObject::new)
            .collect::<Vec<_>>();
        list_store.splice(0, 0, &items);

        if let other @ Self::Other(_) = Self::from_framerate(settings.framerate()) {
            list_store.append(&BoxedAnyObject::new(other));
        }

        list_store
    }

    /// Returns the corresponding `FramerateOption` for the given framerate.
    pub fn from_framerate(framerate: Framerate) -> Self {
        // This must be updated if an option is added or removed.
        let epsilon = Framerate::new_raw(1, 100);

        Self::all_except_other()
            .into_iter()
            .find(|o| (o.as_framerate() - framerate).abs() < epsilon)
            .unwrap_or(Self::Other(framerate))
    }

    /// Converts a `FramerateOption` to a framerate.
    pub const fn as_framerate(self) -> Framerate {
        let (numer, denom) = match self {
            Self::_10 => (10, 1),
            Self::_20 => (20, 1),
            Self::_24 => (24, 1),
            Self::_25 => (25, 1),
            Self::_29_97 => (30_000, 1001),
            Self::_30 => (30, 1),
            Self::_48 => (48, 1),
            Self::_50 => (50, 1),
            Self::_59_94 => (60_000, 1001),
            Self::_60 => (60, 1),
            Self::Other(framerate) => return framerate,
        };
        Framerate::new_raw(numer, denom)
    }
}

impl fmt::Display for FramerateOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::_10 => "10",
            Self::_20 => "20",
            Self::_24 => "24",
            Self::_25 => "25",
            Self::_29_97 => "29.97",
            Self::_30 => "30",
            Self::_48 => "48",
            Self::_50 => "50",
            Self::_59_94 => "59.94",
            Self::_60 => "60",
            Self::Other(framerate) => return write!(f, "{}", framerate),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::pipeline::Framerate;

    use super::*;

    #[track_caller]
    fn test_framerate(framerate: Framerate, expected: FramerateOption) {
        assert_eq!(FramerateOption::from_framerate(framerate), expected);
    }

    #[test]
    fn framerate_option() {
        test_framerate(
            Framerate::from_integer(5),
            FramerateOption::Other(Framerate::from_integer(5)),
        );
        test_framerate(Framerate::from_integer(10), FramerateOption::_10);
        test_framerate(Framerate::from_integer(20), FramerateOption::_20);
        test_framerate(Framerate::from_integer(24), FramerateOption::_24);
        test_framerate(Framerate::from_integer(25), FramerateOption::_25);
        test_framerate(
            Framerate::approximate_float(29.97).unwrap(),
            FramerateOption::_29_97,
        );
        test_framerate(Framerate::from_integer(30), FramerateOption::_30);
        test_framerate(Framerate::from_integer(48), FramerateOption::_48);
        test_framerate(Framerate::from_integer(50), FramerateOption::_50);
        test_framerate(
            Framerate::approximate_float(59.94).unwrap(),
            FramerateOption::_59_94,
        );
        test_framerate(Framerate::from_integer(60), FramerateOption::_60);
        test_framerate(
            Framerate::from_integer(120),
            FramerateOption::Other(Framerate::from_integer(120)),
        );
    }
}
