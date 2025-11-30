pub mod decode;
mod encode;
mod error;
mod value;

pub use decode::decode;
pub use encode::encode;
pub use error::{DecodeError, EncodeError};
pub use value::Value;

#[cfg(test)]
mod tests;
