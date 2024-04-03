use crate::i18n::ngettext_f;

/// Formats a framerate in a more human-readable format.
pub fn framerate(framerate: gst::Fraction) -> String {
    let reduced = framerate.reduced();

    if reduced.is_integer() {
        return reduced.numer().to_string();
    }

    let float = *reduced.numer() as f64 / *reduced.denom() as f64;
    format!("{:.2}", float)
}

/// Formats time in MM:SS.
///
/// The MM part will be more than 2 digits if the time is >= 100 minutes.
pub fn digital_clock(clock_time: gst::ClockTime) -> String {
    let secs = clock_time.seconds();

    let seconds_display = secs % 60;
    let minutes_display = secs / 60;
    format!("{:02}∶{:02}", minutes_display, seconds_display)
}

/// Formats time as duration.
pub fn duration(clock_time: gst::ClockTime) -> String {
    let secs = clock_time.seconds();

    let hours_display = secs / 3600;
    let minutes_display = (secs % 3600) / 60;
    let seconds_display = secs % 60;

    let hours_display_str = ngettext_f(
        // Translators: Do NOT translate the contents between '{' and '}', this is a variable name.
        "{time} hour",
        "{time} hours",
        hours_display as u32,
        &[("time", &hours_display.to_string())],
    );
    let minutes_display_str = ngettext_f(
        // Translators: Do NOT translate the contents between '{' and '}', this is a variable name.
        "{time} minute",
        "{time} minutes",
        minutes_display as u32,
        &[("time", &minutes_display.to_string())],
    );
    let seconds_display_str = ngettext_f(
        // Translators: Do NOT translate the contents between '{' and '}', this is a variable name.
        "{time} second",
        "{time} seconds",
        seconds_display as u32,
        &[("time", &seconds_display.to_string())],
    );

    if hours_display > 0 {
        // 4 hours 5 minutes 6 seconds
        format!(
            "{} {} {}",
            hours_display_str, minutes_display_str, seconds_display_str
        )
    } else if minutes_display > 0 {
        // 5 minutes 6 seconds
        format!("{} {}", minutes_display_str, seconds_display_str)
    } else {
        // 6 seconds
        seconds_display_str
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_framerate() {
        assert_eq!(framerate(gst::Fraction::from_integer(24)), "24");
        assert_eq!(framerate(gst::Fraction::new(30_000, 1001)), "29.97");
        assert_eq!(framerate(gst::Fraction::new(60_000, 1001)), "59.94");
    }

    #[test]
    fn test_duration() {
        assert_eq!(duration(gst::ClockTime::ZERO), "0 seconds");
        assert_eq!(duration(gst::ClockTime::from_seconds(1)), "1 second");
        assert_eq!(
            duration(gst::ClockTime::from_seconds(3 * 60 + 4)),
            "3 minutes 4 seconds"
        );
        assert_eq!(
            duration(gst::ClockTime::from_seconds(60 * 60 + 6)),
            "1 hour 0 minutes 6 seconds"
        );
        assert_eq!(
            duration(gst::ClockTime::from_seconds(2 * 60 * 60)),
            "2 hours 0 minutes 0 seconds"
        );
    }

    #[test]
    fn digital_clock_less_than_1_hour() {
        assert_eq!(digital_clock(gst::ClockTime::ZERO), "00∶00");
        assert_eq!(digital_clock(gst::ClockTime::from_seconds(31)), "00∶31");
        assert_eq!(
            digital_clock(gst::ClockTime::from_seconds(8 * 60 + 1)),
            "08∶01"
        );
        assert_eq!(
            digital_clock(gst::ClockTime::from_seconds(33 * 60 + 3)),
            "33∶03"
        );
        assert_eq!(
            digital_clock(gst::ClockTime::from_seconds(59 * 60 + 59)),
            "59∶59"
        );
    }

    #[test]
    fn digital_clock_more_than_1_hour() {
        assert_eq!(
            digital_clock(gst::ClockTime::from_seconds(60 * 60)),
            "60∶00"
        );
        assert_eq!(
            digital_clock(gst::ClockTime::from_seconds(60 * 60 + 9)),
            "60∶09"
        );
        assert_eq!(
            digital_clock(gst::ClockTime::from_seconds(60 * 60 + 31)),
            "60∶31"
        );
        assert_eq!(
            digital_clock(gst::ClockTime::from_seconds(100 * 60 + 20)),
            "100∶20"
        );
    }
}
