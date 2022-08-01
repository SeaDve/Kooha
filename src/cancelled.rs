#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Cancelled(Option<String>);

impl Cancelled {
    pub fn new(message: &str) -> Self {
        Self(Some(message.to_string()))
    }
}

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref message) = self.0 {
            f.write_str(message)
        } else {
            f.write_str("Operation was cancelled")
        }
    }
}

impl std::error::Error for Cancelled {}
