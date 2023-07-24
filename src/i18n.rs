// Copied from Fractal GPLv3
// See https://gitlab.gnome.org/GNOME/fractal/-/blob/c0bc4078bb2cdd511c89fdf41a51275db90bb7ab/src/i18n.rs

use gettextrs::gettext;

/// Like `gettext`, but replaces named variables using the given key-value tuples.
///
/// The expected format to replace is `{name}`, where `name` is the first string
/// in a key-value tuple.
pub fn gettext_f(msgid: &str, args: &[(&str, &str)]) -> String {
    let s = gettext(msgid);
    freplace(s, args)
}

/// Replace variables in the given string using the given key-value tuples.
///
/// The expected format to replace is `{name}`, where `name` is the first string
/// in a key-value tuple.
fn freplace(s: String, args: &[(&str, &str)]) -> String {
    // This function is useless if there are no arguments
    debug_assert!(!args.is_empty(), "atleast one key-value pair must be given");

    // We could check here if all keys were used, but some translations might
    // not use all variables, so we don't do that.

    let mut s = s;
    for (key, val) in args {
        s = s.replace(&format!("{{{key}}}"), val);
    }

    debug_assert!(!s.contains('{'), "all format variables must be replaced");

    if tracing::enabled!(tracing::Level::WARN) && s.contains('{') {
        tracing::warn!(
            "all format variables must be replaced, but some were not: {}",
            s
        );
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic = "atleast one key-value pair must be given"]
    fn freplace_no_args() {
        gettext_f("no args", &[]);
    }

    #[test]
    #[should_panic = "all format variables must be replaced"]
    fn freplace_missing_key() {
        gettext_f("missing {one}", &[("two", "2")]);
    }

    #[test]
    fn gettext_f_simple() {
        assert_eq!(gettext_f("no replace", &[("one", "1")]), "no replace");
        assert_eq!(gettext_f("{one} param", &[("one", "1")]), "1 param");
        assert_eq!(
            gettext_f("middle {one} param", &[("one", "1")]),
            "middle 1 param"
        );
        assert_eq!(gettext_f("end {one}", &[("one", "1")]), "end 1");
    }

    #[test]
    fn gettext_f_multiple() {
        assert_eq!(
            gettext_f("multiple {one} and {two}", &[("one", "1"), ("two", "2")]),
            "multiple 1 and 2"
        );
        assert_eq!(
            gettext_f("multiple {two} and {one}", &[("one", "1"), ("two", "2")]),
            "multiple 2 and 1"
        );
        assert_eq!(
            gettext_f("multiple {one} and {one}", &[("one", "1"), ("two", "2")]),
            "multiple 1 and 1"
        );
    }
}
