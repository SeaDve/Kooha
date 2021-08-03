#[macro_export]
macro_rules! i18n {
    ($string:tt) => {
        gettextrs::gettext($string)
    };

    ($string:tt, $($arg:expr),*) => ({
        {
            let translated = gettextrs::gettext($string);
            let mut parts = translated.split("{}");
            let mut output = parts.next().unwrap_or("").to_string();
            $(
                output += &($arg.to_string() + &parts.next().unwrap_or("").to_string());
            )*
            output
        }
    })
}

#[macro_export]
macro_rules! ni18n {
    (s $single:tt, p $multiple:tt, n $number:expr) => {
        gettextrs::ngettext($single, $multiple, $number)
    };

    (s $single:tt, p $multiple:tt, n $number:expr, $($arg:expr),*) => {
        {
            let translated = gettextrs::ngettext($single, $multiple, $number);
            let mut parts = translated.split("{}");
            let mut output = parts.next().unwrap_or("").to_string();
            $(
                output += &($arg.to_string() + &parts.next().unwrap_or("").to_string());
            )*
            output
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_i18n() {
        let out = i18n!("translate1");
        assert_eq!(out, "translate1");

        let out = ni18n!(s "translate1", p "translate multi", n 1);
        assert_eq!(out, "translate1");
        let out = ni18n!(s "translate1", p "translate multi", n 2);
        assert_eq!(out, "translate multi");
    }

    #[test]
    fn test_i18n_f() {
        let out = i18n!("{} param", "one");
        assert_eq!(out, "one param");

        let out = i18n!("middle {} param", "one");
        assert_eq!(out, "middle one param");

        let out = i18n!("end {}", "one");
        assert_eq!(out, "end one");

        let out = i18n!("multiple {} and {}", "one", "two");
        assert_eq!(out, "multiple one and two");

        let out = ni18n!(s "singular {} and {}", p "plural {} and {}", n 2, "one", "two");
        assert_eq!(out, "plural one and two");
        let out = ni18n!(s "singular {} and {}", p "plural {} and {}", n 1, "one", "two");
        assert_eq!(out, "singular one and two");
    }
}
