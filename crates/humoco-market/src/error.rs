use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum MarketError {
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Identity error: {0}")]
    Identity(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Date parsing error: {0}")]
    ParseDate(String),
    #[error("Cryptographic error: {0}")]
    Crypto(String),
    #[error("Invalid card JWS: {0}")]
    InvalidJws(String),
    #[error("Slot error: {0}")]
    Slot(String),
    #[error("Payload too large: {0}")]
    PayloadTooLarge(String),
}
