use std::fmt;

use gtk::glib;
use num_traits::Signed;

use crate::pipeline::Framerate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "KoohaFramerateOption")]
pub enum FramerateOption {
    _10,
    _20,
    _23_976,
    _24,
    _25,
    _29_97,
    _30,
    _48,
    _50,
    _59_94,
    _60,
}

impl FramerateOption {
    /// Returns the closest `FramerateOption` to the given `Framerate`.
    pub fn from_framerate(framerate: Framerate) -> Self {
        let all = [
            Self::_10,
            Self::_20,
            Self::_23_976,
            Self::_24,
            Self::_25,
            Self::_29_97,
            Self::_30,
            Self::_48,
            Self::_50,
            Self::_59_94,
            Self::_60,
        ];

        *all.iter()
            .min_by(|a, b| {
                (a.to_framerate() - framerate)
                    .abs()
                    .cmp(&(b.to_framerate() - framerate).abs())
            })
            .unwrap()
    }

    /// Converts a `FramerateOption` to a `Framerate`.
    pub fn to_framerate(self) -> Framerate {
        let (numer, denom) = match self {
            Self::_10 => (10, 1),
            Self::_20 => (20, 1),
            Self::_23_976 => (24_000, 1001),
            Self::_24 => (24, 1),
            Self::_25 => (25, 1),
            Self::_29_97 => (30_000, 1001),
            Self::_30 => (30, 1),
            Self::_48 => (48, 1),
            Self::_50 => (50, 1),
            Self::_59_94 => (60_000, 1001),
            Self::_60 => (60, 1),
        };
        Framerate::new(numer, denom)
    }
}

impl fmt::Display for FramerateOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::_10 => "10",
            Self::_20 => "20",
            Self::_23_976 => "23.976",
            Self::_24 => "24 NTSC",
            Self::_25 => "25 PAL",
            Self::_29_97 => "29.97",
            Self::_30 => "30",
            Self::_48 => "48",
            Self::_50 => "50 PAL",
            Self::_59_94 => "59.94",
            Self::_60 => "60",
        };
        f.write_str(name)
    }
}
