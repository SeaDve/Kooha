use error_stack::{Report, Result};

use std::fmt;

pub struct Help(String);

impl Help {
    fn new(msg: impl Into<String>) -> Self {
        Help(msg.into())
    }
}

impl fmt::Display for Help {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub trait ReportExt {
    fn attach_help(self, msg: &str) -> Self;
}

impl<C> ReportExt for Report<C> {
    fn attach_help(self, msg: &str) -> Self {
        self.attach(Help::new(msg))
    }
}

pub trait ResultExt {
    fn attach_help(self, msg: &str) -> Self;
    fn attach_help_lazy<F, O>(self, msg_func: F) -> Self
    where
        F: FnOnce() -> O,
        O: Into<String>;
}

impl<T, C> ResultExt for Result<T, C> {
    #[track_caller]
    fn attach_help(self, msg: &str) -> Self {
        match self {
            Ok(ok) => Ok(ok),
            Err(report) => Err(report.attach_help(msg)),
        }
    }

    #[track_caller]
    fn attach_help_lazy<F, O>(self, msg_func: F) -> Self
    where
        F: FnOnce() -> O,
        O: Into<String>,
    {
        match self {
            Ok(ok) => Ok(ok),
            Err(report) => Err(report.attach(Help::new(msg_func().into()))),
        }
    }
}
