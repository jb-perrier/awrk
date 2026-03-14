pub mod decode;
pub mod encode;
pub mod tags;

pub use decode::{DecodeConfig, decode_value, decode_value_full};
pub use encode::{
    EncodeConfig, Encoder, MapWriter, SeqWriter, encode_value, encode_value_into,
    encode_value_into_with_config,
};
