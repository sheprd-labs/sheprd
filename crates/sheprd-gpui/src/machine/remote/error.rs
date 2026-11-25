use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("The answer from {host} was not one Sheprd sends: {source}")]
    Answer {
        host: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("The picture from the other computer is not valid base64: {0}")]
    Base64(#[source] base64::DecodeError),
    #[error("Could not read the screenshot: {0}")]
    Decode(#[source] image::ImageError),
    #[error("Could not encode the frame: {0}")]
    Encode(#[source] image::ImageError),
    #[error("No Sheprd is installed on {0}")]
    Missing(String),
    #[error("The picture from the other computer is not a PNG of the size it says")]
    NotPng,
    #[error("Sheprd is not running on {0}, so its virtual Macs cannot start")]
    NotRunning(String),
    #[error("{host} answered: {message}")]
    Refused { host: String, message: String },
    #[error("The request to {host} could not be written: {source}")]
    Request {
        host: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("Could not reach {host}: {source}")]
    Run {
        host: String,
        #[source]
        source: sheprd_client::Error,
    },
    #[error("The answer from {0} is larger than a picture can be")]
    TooLarge(String),
    #[error("Sheprd on {0} is too old to lend virtual Macs; update it there")]
    TooOld(String),
    #[error("Cannot write {}: {source}", path.display())]
    Write {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}
