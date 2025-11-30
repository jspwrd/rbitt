use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("unexpected end of input")]
    UnexpectedEof,

    #[error("invalid integer: {0}")]
    InvalidInteger(String),

    #[error("invalid string length")]
    InvalidStringLength,

    #[error("expected '{expected}' but found '{found}'")]
    UnexpectedByte { expected: char, found: char },

    #[error("dictionary keys must be strings")]
    NonStringKey,

    #[error("dictionary keys must be sorted")]
    UnsortedKeys,

    #[error("trailing data after value")]
    TrailingData,

    #[error("invalid utf-8 in string")]
    InvalidUtf8,

    #[error("integer with leading zeros")]
    LeadingZeros,

    #[error("negative zero is not allowed")]
    NegativeZero,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EncodeError {
    #[error("dictionary keys must be sorted byte-wise")]
    UnsortedKeys,
}
