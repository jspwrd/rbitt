use crate::error::DecodeError;
use crate::value::Value;
use bytes::Bytes;
use std::collections::BTreeMap;

pub fn decode(data: &[u8]) -> Result<Value, DecodeError> {
    let mut cursor = Cursor::new(data);
    let value = decode_value(&mut cursor)?;

    if cursor.remaining() > 0 {
        return Err(DecodeError::TrailingData);
    }

    Ok(value)
}

pub fn decode_with_info_raw(data: &[u8]) -> Result<(Value, Option<Bytes>), DecodeError> {
    let mut cursor = Cursor::new(data);
    let (value, info_raw) = decode_value_tracking_info(&mut cursor)?;

    if cursor.remaining() > 0 {
        return Err(DecodeError::TrailingData);
    }

    Ok((value, info_raw))
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn next(&mut self) -> Result<u8, DecodeError> {
        let byte = self.peek().ok_or(DecodeError::UnexpectedEof)?;
        self.pos += 1;
        Ok(byte)
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn position(&self) -> usize {
        self.pos
    }

    fn slice_from(&self, start: usize) -> &'a [u8] {
        &self.data[start..self.pos]
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], DecodeError> {
        if self.pos + len > self.data.len() {
            return Err(DecodeError::UnexpectedEof);
        }
        let bytes = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(bytes)
    }
}

fn decode_value(cursor: &mut Cursor) -> Result<Value, DecodeError> {
    let (value, _) = decode_value_tracking_info(cursor)?;
    Ok(value)
}

fn decode_value_tracking_info(cursor: &mut Cursor) -> Result<(Value, Option<Bytes>), DecodeError> {
    match cursor.peek().ok_or(DecodeError::UnexpectedEof)? {
        b'i' => Ok((decode_integer(cursor)?, None)),
        b'l' => Ok((decode_list(cursor)?, None)),
        b'd' => decode_dict_tracking_info(cursor),
        b'0'..=b'9' => Ok((decode_bytes(cursor)?, None)),
        b => Err(DecodeError::UnexpectedByte {
            expected: 'i',
            found: b as char,
        }),
    }
}

fn decode_integer(cursor: &mut Cursor) -> Result<Value, DecodeError> {
    let start = cursor.next()?;
    debug_assert_eq!(start, b'i');

    let mut num_bytes = Vec::new();
    loop {
        let b = cursor.next()?;
        if b == b'e' {
            break;
        }
        num_bytes.push(b);
    }

    if num_bytes.is_empty() {
        return Err(DecodeError::InvalidInteger("empty".to_string()));
    }

    let s = std::str::from_utf8(&num_bytes)
        .map_err(|_| DecodeError::InvalidInteger("invalid utf-8".to_string()))?;

    if s == "-0" {
        return Err(DecodeError::NegativeZero);
    }
    if s.len() > 1 && s.starts_with('0') {
        return Err(DecodeError::LeadingZeros);
    }
    if s.len() > 2 && s.starts_with("-0") {
        return Err(DecodeError::LeadingZeros);
    }

    let num: i64 = s
        .parse()
        .map_err(|_| DecodeError::InvalidInteger(s.to_string()))?;

    Ok(Value::Integer(num))
}

fn decode_bytes(cursor: &mut Cursor) -> Result<Value, DecodeError> {
    let mut len_bytes = Vec::new();

    loop {
        let b = cursor.next()?;
        if b == b':' {
            break;
        }
        if !b.is_ascii_digit() {
            return Err(DecodeError::InvalidStringLength);
        }
        len_bytes.push(b);
    }

    if len_bytes.is_empty() {
        return Err(DecodeError::InvalidStringLength);
    }

    if len_bytes.len() > 1 && len_bytes[0] == b'0' {
        return Err(DecodeError::LeadingZeros);
    }

    let len_str = std::str::from_utf8(&len_bytes).map_err(|_| DecodeError::InvalidStringLength)?;
    let len: usize = len_str
        .parse()
        .map_err(|_| DecodeError::InvalidStringLength)?;

    let bytes = cursor.read_bytes(len)?;
    Ok(Value::Bytes(Bytes::copy_from_slice(bytes)))
}

fn decode_list(cursor: &mut Cursor) -> Result<Value, DecodeError> {
    let start = cursor.next()?;
    debug_assert_eq!(start, b'l');

    let mut items = Vec::new();

    loop {
        if cursor.peek() == Some(b'e') {
            cursor.next()?;
            break;
        }
        items.push(decode_value(cursor)?);
    }

    Ok(Value::List(items))
}

fn decode_dict_tracking_info(cursor: &mut Cursor) -> Result<(Value, Option<Bytes>), DecodeError> {
    let start = cursor.next()?;
    debug_assert_eq!(start, b'd');

    let mut dict = BTreeMap::new();
    let mut last_key: Option<Bytes> = None;
    let mut info_raw: Option<Bytes> = None;

    loop {
        if cursor.peek() == Some(b'e') {
            cursor.next()?;
            break;
        }

        let key = match decode_value(cursor)? {
            Value::Bytes(b) => b,
            _ => return Err(DecodeError::NonStringKey),
        };

        if let Some(ref prev) = last_key {
            if key.as_ref() <= prev.as_ref() {
                return Err(DecodeError::UnsortedKeys);
            }
        }
        last_key = Some(key.clone());

        let is_info = key.as_ref() == b"info";
        let value_start = cursor.position();

        let value = decode_value(cursor)?;

        if is_info {
            let raw = cursor.slice_from(value_start);
            info_raw = Some(Bytes::copy_from_slice(raw));
        }

        dict.insert(key, value);
    }

    Ok((Value::Dict(dict), info_raw))
}
