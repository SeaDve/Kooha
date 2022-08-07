use std::{error, fmt};

#[derive(Debug)]
pub struct Cancelled {
    task: String,
}

impl Cancelled {
    pub fn new(task: impl Into<String>) -> Self {
        Cancelled { task: task.into() }
    }
}

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cancelled {}", self.task)
    }
}

impl error::Error for Cancelled {}
