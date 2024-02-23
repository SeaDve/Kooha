use std::fmt;

#[derive(Debug)]
pub struct ContextWithHelp {
    context: String,
    help_message: String,
}

impl ContextWithHelp {
    pub fn new(context: impl Into<String>, help_message: impl Into<String>) -> Self {
        ContextWithHelp {
            context: context.into(),
            help_message: help_message.into(),
        }
    }

    pub fn help_message(&self) -> &str {
        &self.help_message
    }
}

impl fmt::Display for ContextWithHelp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.context)
    }
}
