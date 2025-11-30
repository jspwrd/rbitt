use crate::error::EncodeError;
use crate::value::Value;
use bytes::{BufMut, BytesMut};

pub fn encode(value: &Value) -> Result<Vec<u8>, EncodeError> {
    let mut buf = BytesMut::new();
    encode_value(value, &mut buf)?;
    Ok(buf.to_vec())
}

fn encode_value(value: &Value, buf: &mut BytesMut) -> Result<(), EncodeError> {
    match value {
        Value::Bytes(b) => {
            encode_bytes(b, buf);
            Ok(())
        }
        Value::Integer(i) => {
            encode_integer(*i, buf);
            Ok(())
        }
        Value::List(items) => encode_list(items, buf),
        Value::Dict(dict) => encode_dict(dict, buf),
    }
}

fn encode_bytes(bytes: &[u8], buf: &mut BytesMut) {
    let len_str = bytes.len().to_string();
    buf.put_slice(len_str.as_bytes());
    buf.put_u8(b':');
    buf.put_slice(bytes);
}

fn encode_integer(i: i64, buf: &mut BytesMut) {
    buf.put_u8(b'i');
    let num_str = i.to_string();
    buf.put_slice(num_str.as_bytes());
    buf.put_u8(b'e');
}

fn encode_list(items: &[Value], buf: &mut BytesMut) -> Result<(), EncodeError> {
    buf.put_u8(b'l');
    for item in items {
        encode_value(item, buf)?;
    }
    buf.put_u8(b'e');
    Ok(())
}

fn encode_dict(
    dict: &std::collections::BTreeMap<bytes::Bytes, Value>,
    buf: &mut BytesMut,
) -> Result<(), EncodeError> {
    buf.put_u8(b'd');

    let mut prev_key: Option<&[u8]> = None;
    for (key, value) in dict.iter() {
        if let Some(prev) = prev_key {
            if key.as_ref() <= prev {
                return Err(EncodeError::UnsortedKeys);
            }
        }
        prev_key = Some(key.as_ref());

        encode_bytes(key, buf);
        encode_value(value, buf)?;
    }

    buf.put_u8(b'e');
    Ok(())
}
