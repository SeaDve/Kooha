use ashpd::desktop::ResponseError;
use gtk::glib;

#[derive(Debug, Clone)]
pub enum Error {
    Portal(String),
    Recorder(glib::Error),
    Pipeline(glib::Error),
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Portal(e) => f.write_str(&format!("Screencast portal request failed: {}", e)),
            Self::Recorder(e) => f.write_str(&format!("Record failed: {}", e)),
            Self::Pipeline(e) => f.write_str(&format!("Pipeline build failed: {}", e)),
        }
    }
}

impl From<ResponseError> for Error {
    fn from(e: ResponseError) -> Self {
        Self::Portal(e.to_string())
    }
}

impl From<&ashpd::Error> for Error {
    fn from(e: &ashpd::Error) -> Self {
        Self::Portal(e.to_string())
    }
}
