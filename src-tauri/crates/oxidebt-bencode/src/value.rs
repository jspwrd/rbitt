use bytes::Bytes;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Bytes(Bytes),
    Integer(i64),
    List(Vec<Value>),
    Dict(BTreeMap<Bytes, Value>),
}

impl Value {
    pub fn string(s: &str) -> Self {
        Value::Bytes(Bytes::copy_from_slice(s.as_bytes()))
    }

    pub fn bytes(b: impl Into<Bytes>) -> Self {
        Value::Bytes(b.into())
    }

    pub fn integer(i: i64) -> Self {
        Value::Integer(i)
    }

    pub fn list(items: Vec<Value>) -> Self {
        Value::List(items)
    }

    pub fn dict(items: BTreeMap<Bytes, Value>) -> Self {
        Value::Dict(items)
    }

    pub fn as_bytes(&self) -> Option<&Bytes> {
        match self {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Bytes(b) => std::str::from_utf8(b).ok(),
            _ => None,
        }
    }

    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Value::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&Vec<Value>> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_dict(&self) -> Option<&BTreeMap<Bytes, Value>> {
        match self {
            Value::Dict(d) => Some(d),
            _ => None,
        }
    }

    pub fn as_dict_mut(&mut self) -> Option<&mut BTreeMap<Bytes, Value>> {
        match self {
            Value::Dict(d) => Some(d),
            _ => None,
        }
    }

    pub fn get(&self, key: &[u8]) -> Option<&Value> {
        match self {
            Value::Dict(d) => d.get(key),
            _ => None,
        }
    }

    pub fn get_str(&self, key: &str) -> Option<&Value> {
        self.get(key.as_bytes())
    }

    pub fn is_bytes(&self) -> bool {
        matches!(self, Value::Bytes(_))
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, Value::Integer(_))
    }

    pub fn is_list(&self) -> bool {
        matches!(self, Value::List(_))
    }

    pub fn is_dict(&self) -> bool {
        matches!(self, Value::Dict(_))
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::string(s)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Bytes(Bytes::from(s))
    }
}

impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Value::Bytes(Bytes::from(v))
    }
}

impl From<Bytes> for Value {
    fn from(b: Bytes) -> Self {
        Value::Bytes(b)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Integer(i)
    }
}

impl From<i32> for Value {
    fn from(i: i32) -> Self {
        Value::Integer(i64::from(i))
    }
}

impl From<u32> for Value {
    fn from(i: u32) -> Self {
        Value::Integer(i64::from(i))
    }
}

impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Value::List(v)
    }
}

impl From<BTreeMap<Bytes, Value>> for Value {
    fn from(m: BTreeMap<Bytes, Value>) -> Self {
        Value::Dict(m)
    }
}
