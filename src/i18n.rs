use gettextrs::gettext;
use gettextrs::ngettext;
use regex::Captures;
use regex::Regex;

#[allow(dead_code)]
fn freplace(input: String, args: &[&str]) -> String {
    let mut parts = input.split("{}");
    let mut output = parts.next().unwrap_or("").to_string();
    for (p, a) in parts.zip(args.iter()) {
        output += &(a.to_string() + &p.to_string());
    }
    output
}

#[allow(dead_code)]
fn kreplace(input: String, kwargs: &[(&str, &str)]) -> String {
    let mut s = input;
    for (k, v) in kwargs {
        if let Ok(re) = Regex::new(&format!("\\{{{}\\}}", k)) {
            s = re
                .replace_all(&s, |_: &Captures<'_>| v.to_string())
                .to_string();
        }
    }

    s
}

#[allow(dead_code)]
pub fn i18n(format: &str) -> String {
    gettext(format)
}

#[allow(dead_code)]
pub fn i18n_f(format: &str, args: &[&str]) -> String {
    let s = gettext(format);
    freplace(s, args)
}

#[allow(dead_code)]
pub fn i18n_k(format: &str, kwargs: &[(&str, &str)]) -> String {
    let s = gettext(format);
    kreplace(s, kwargs)
}

#[allow(dead_code)]
pub fn ni18n(single: &str, multiple: &str, number: u32) -> String {
    ngettext(single, multiple, number)
}

#[allow(dead_code)]
pub fn ni18n_f(single: &str, multiple: &str, number: u32, args: &[&str]) -> String {
    let s = ngettext(single, multiple, number);
    freplace(s, args)
}

#[allow(dead_code)]
pub fn ni18n_k(single: &str, multiple: &str, number: u32, kwargs: &[(&str, &str)]) -> String {
    let s = ngettext(single, multiple, number);
    kreplace(s, kwargs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_i18n() {
        let out = i18n("translate1");
        assert_eq!(out, "translate1");

        let out = ni18n("translate1", "translate multi", 1);
        assert_eq!(out, "translate1");
        let out = ni18n("translate1", "translate multi", 2);
        assert_eq!(out, "translate multi");
    }

    #[test]
    fn test_i18n_f() {
        let out = i18n_f("{} param", &["one"]);
        assert_eq!(out, "one param");

        let out = i18n_f("middle {} param", &["one"]);
        assert_eq!(out, "middle one param");

        let out = i18n_f("end {}", &["one"]);
        assert_eq!(out, "end one");

        let out = i18n_f("multiple {} and {}", &["one", "two"]);
        assert_eq!(out, "multiple one and two");

        let out = ni18n_f("singular {} and {}", "plural {} and {}", 2, &["one", "two"]);
        assert_eq!(out, "plural one and two");
        let out = ni18n_f("singular {} and {}", "plural {} and {}", 1, &["one", "two"]);
        assert_eq!(out, "singular one and two");
    }

    #[test]
    fn test_i18n_k() {
        let out = i18n_k("{one} param", &[("one", "one")]);
        assert_eq!(out, "one param");

        let out = i18n_k("middle {one} param", &[("one", "one")]);
        assert_eq!(out, "middle one param");

        let out = i18n_k("end {one}", &[("one", "one")]);
        assert_eq!(out, "end one");

        let out = i18n_k("multiple {one} and {two}", &[("one", "1"), ("two", "two")]);
        assert_eq!(out, "multiple 1 and two");

        let out = i18n_k("multiple {two} and {one}", &[("one", "1"), ("two", "two")]);
        assert_eq!(out, "multiple two and 1");

        let out = i18n_k("multiple {one} and {one}", &[("one", "1"), ("two", "two")]);
        assert_eq!(out, "multiple 1 and 1");

        let out = ni18n_k(
            "singular {one} and {two}",
            "plural {one} and {two}",
            1,
            &[("one", "1"), ("two", "two")],
        );
        assert_eq!(out, "singular 1 and two");
        let out = ni18n_k(
            "singular {one} and {two}",
            "plural {one} and {two}",
            2,
            &[("one", "1"), ("two", "two")],
        );
        assert_eq!(out, "plural 1 and two");
    }
}
