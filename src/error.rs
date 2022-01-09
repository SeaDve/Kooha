use ashpd::desktop::ResponseError;
use gettextrs::gettext;
use gtk::glib;

#[derive(Debug, Clone)]
pub enum Error {
    Portal(String),
    Pipeline(glib::Error),
    Recorder(glib::Error),
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Portal(_) => f.write_str(&gettext("Screencast Portal Request Error")),
            Self::Pipeline(_) => f.write_str(&gettext("Pipeline Build Error")),
            Self::Recorder(_) => f.write_str(&gettext("Recording Error")),
        }
    }
}

impl From<ResponseError> for Error {
    fn from(e: ResponseError) -> Self {
        Self::Portal(e.to_string())
    }
}

impl From<ashpd::Error> for Error {
    fn from(e: ashpd::Error) -> Self {
        Self::Portal(e.to_string())
    }
}

impl Error {
    fn message(&self) -> String {
        match self {
            Self::Portal(e) => e.to_string(),
            Self::Pipeline(e) => e.to_string(),
            Self::Recorder(e) => e.to_string(),
        }
    }

    pub fn help(&self) -> String {
        let help_message = match self {
            Self::Portal(_) => gettext("Make sure to check for the runtime dependencies and <a href=\"https://github.com/SeaDve/Kooha#-it-doesnt-work\">It Doesn't Work page</a>."),
            Self::Pipeline(_) => gettext("A GStreamer plugin may not be installed. If it is installed but still does not work properly, please report to <a href=\"https://github.com/SeaDve/Kooha/issues\">Kooha's issue page</a>."),
            Self::Recorder(_) => gettext("Make sure that the saving location exists or is accessible. If it actually exists or is accessible, something went wrong and please report to <a href=\"https://github.com/SeaDve/Kooha/issues\">Kooha's issue page</a>."),
        };

        format!("{}\n\n<b>Help</b>: {}", self.message(), help_message)
    }
}
