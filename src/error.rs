use ashpd::desktop::ResponseError;
use gettextrs::gettext;
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
            Self::Portal(e) => f.write_str(&gettext!("Make sure to check for the runtime dependencies and \"It Doesn't Work\" page in Kooha's Github page. ({})", e)),
            Self::Recorder(e) => f.write_str(&format!("{}", e)),
            Self::Pipeline(e) => f.write_str(&gettext!("Please report to the issue page. ({})", e)),
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

impl Error {
    pub fn title(&self) -> String {
        match self {
            Self::Portal(_) => gettext("Screencast Portal Request Failed"),
            Self::Recorder(_) => gettext("Recording Failed"),
            Self::Pipeline(_) => gettext("Pipeline Build Failed"),
        }
    }
}
