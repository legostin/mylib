use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("xml: {0}")]
    Xml(#[from] quick_xml::Error),

    #[error("base64: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("inpx format: {0}")]
    InpxFormat(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("{0}")]
    Other(String),
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
