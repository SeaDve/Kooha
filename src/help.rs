use anyhow::{Context, Error, Result};

use std::fmt;

#[derive(Debug)]
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

pub trait ErrorExt {
    fn help(
        self,
        help_msg: impl Into<String>,
        context: impl fmt::Display + Send + Sync + 'static,
    ) -> Self;
}

impl ErrorExt for Error {
    fn help(
        self,
        help_msg: impl Into<String>,
        context: impl fmt::Display + Send + Sync + 'static,
    ) -> Self {
        self.context(Help::new(help_msg)).context(context)
    }
}

pub trait ResultExt<T> {
    fn help<M, C>(self, help_msg: M, context: C) -> Result<T>
    where
        M: Into<String>,
        C: fmt::Display + Send + Sync + 'static;

    fn with_help<M, C>(
        self,
        help_msg: impl FnOnce() -> M,
        context_fn: impl FnOnce() -> C,
    ) -> Result<T>
    where
        M: Into<String>,
        C: fmt::Display + Send + Sync + 'static;
}

impl<T> ResultExt<T> for Result<T> {
    fn help<M, C>(self, help_msg: M, context: C) -> Result<T>
    where
        M: Into<String>,
        C: fmt::Display + Send + Sync + 'static,
    {
        self.context(Help::new(help_msg)).context(context)
    }

    fn with_help<M, C>(
        self,
        help_msg_fn: impl FnOnce() -> M,
        context_fn: impl FnOnce() -> C,
    ) -> Result<T>
    where
        M: Into<String>,
        C: fmt::Display + Send + Sync + 'static,
    {
        self.with_context(|| Help::new(help_msg_fn()))
            .with_context(context_fn)
    }
}
