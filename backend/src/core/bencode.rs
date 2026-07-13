use std::collections::BTreeMap;
use crate::core::error::{BError, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BencodeValue {
    // Integer: i<number>e
    Integer(i64),
    // ByteString: <length>:<data>
    ByteString(Vec<u8>),
    // List: l<items>e
    List(Vec<BencodeValue>),
    // Dictionary: d<key1><val1><key2><val2>...e (key sorted)
    Dict(BTreeMap<Vec<u8>, BencodeValue>),
}

impl BencodeValue {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.encode_into(&mut buf);
        buf
    }

    pub fn decode(input: &[u8]) -> Result<(Self, usize)> {
        let mut pos = 0;
        let value = Self::parse_value(input, &mut pos)?;
        Ok((value, pos))
    }

    pub fn dict_get(&self, key: &[u8]) -> Option<&BencodeValue> {
        match self {
            BencodeValue::Dict(map) => map.get(key),
            _ => None
        }
    }

    pub fn dict_get_str(&self, key: &[u8]) -> Option<String> {
        self.dict_get(key).and_then(|v| v.as_str().ok())
    }

    pub fn dict_get_int(&self, key: &[u8]) -> Option<i64> {
        self.dict_get(key).and_then(|v| v.as_int())
    }

    pub fn dict_get_bytes(&self, key: &[u8]) -> Option<&[u8]> {
        self.dict_get(key).and_then(|v| v.as_bytes())
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            BencodeValue::Integer(i) => Some(*i),
            _ => None
        }
    }

    pub fn as_str(&self) -> Result<String> {
        match self {
            BencodeValue::ByteString(data) => {
                String::from_utf8(data.clone())
                    .map_err(|_| BError::BencodeParse("byte string is not valid UTF-8".into()))
            }
            _ => Err(BError::BencodeParse(format!(
                "expected ByteString, got {:?}", self
            )))
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            BencodeValue::ByteString(data) => Some(data),
            _ => None
        }
    }

    fn parse_value(input: &[u8], pos: &mut usize) -> Result<Self> {
        match input.get(*pos) {
            Some(b'i') => Self::parse_integer(input, pos),
            Some(b'l') => Self::parse_list(input, pos),
            Some(b'd') => Self::parse_dict(input, pos),
            Some(b'0'..=b'9') => Self::parse_byte_string(input, pos),
            Some(&c) => Err(BError::BencodeParse(format!(
                "unexpected byte '{}' at position {}",
                c as char, pos
            ))),
            None => Err(BError::BencodeUnexpectedEof),
        }
    }

    fn parse_integer(input: &[u8], pos: &mut usize) -> Result<Self> {
        *pos += 1;
        let start = *pos;
        while *pos < input.len() && input[*pos] != b'e' {
            *pos += 1;
        }
        if *pos >= input.len() {
            return Err(BError::BencodeUnexpectedEof);
        }

        let num_str = std::str::from_utf8(&input[start..*pos])
            .map_err(|_| BError::BencodeParse("invalid integer encoding".into()))?;

        let num: i64 = num_str
            .parse()
            .map_err(|_| BError::BencodeParse(format!("invalide integer: {}", num_str)))?;

        *pos += 1;
        Ok(BencodeValue::Integer(num))
    }

    fn parse_byte_string(input: &[u8], pos: &mut usize) -> Result<Self> {
        let start = *pos;

        while *pos < input.len() && input[*pos] != b':' {
            if !input[*pos].is_ascii_digit() {
                return Err(BError::BencodeParse(format!(
                    "expected digit in length prefix at position {}",
                    *pos
                )));
            }
            *pos += 1;
        }

        if *pos >= input.len() {
            return Err(BError::BencodeUnexpectedEof);
        }

        let len_str = std::str::from_utf8(&input[start..*pos])
            .map_err(|_| BError::BencodeParse("invalid length prefix encoding".into()))?;

        let len: usize = len_str
            .parse()
            .map_err(|_| BError::BencodeParse(format!("invalid length: {}", len_str)))?;

        *pos += 1;

        let data_start = *pos;
        let data_end = data_start + len;
        if data_end > input.len() {
            return Err(BError::BencodeUnexpectedEof);
        }

        *pos = data_end;

        Ok(BencodeValue::ByteString(input[data_start..data_end].to_vec()))
    }

    fn parse_list(input: &[u8], pos: &mut usize) -> Result<Self> {
        *pos += 1;
        let mut items = Vec::new();
        while *pos < input.len() && input[*pos] != b'e' {
            let value = Self::parse_value(input, pos)?;
            items.push(value);
        }
        if *pos >= input.len() {
            return Err(BError::BencodeUnexpectedEof);
        }
        *pos += 1;
        Ok(BencodeValue::List(items))
    }

    fn parse_dict(input: &[u8], pos: &mut usize) -> Result<Self> {
        *pos += 1;
        let mut map = BTreeMap::new();
        while *pos < input.len() && input[*pos] != b'e' {
            let key = match Self::parse_value(input, pos)? {
                BencodeValue::ByteString(k) => k,
                other => {
                    return Err(BError::BencodeParse(format!(
                        "expected byte string as dict key, got {:?}", other
                    )))
                }
            };

            let value = Self::parse_value(input, pos)?;
            map.insert(key, value);
        }
        if *pos >= input.len() {
            return Err(BError::BencodeUnexpectedEof);
        }
        *pos += 1;
        Ok(BencodeValue::Dict(map))
    }

    fn encode_into(&self, buf: &mut Vec<u8>) {
        match self {
            BencodeValue::Integer(i) => {
                buf.extend_from_slice(b"i");
                buf.extend_from_slice(i.to_string().as_bytes());
                buf.extend_from_slice(b"e");
            }
            BencodeValue::ByteString(data) => {
                buf.extend_from_slice(data.len().to_string().as_bytes());
                buf.extend_from_slice(b":");
                buf.extend_from_slice(data);
            }
            BencodeValue::List(items) => {
                buf.extend_from_slice(b"l");
                for item in items {
                    item.encode_into(buf);
                }
                buf.extend_from_slice(b"e");
            }
            BencodeValue::Dict(dict) => {
                buf.extend_from_slice(b"d");
                for (key, value) in dict {
                    buf.extend_from_slice(key.len().to_string().as_bytes());
                    buf.extend_from_slice(b":");
                    buf.extend_from_slice(key);
                    value.encode_into(buf);
                }
                buf.extend_from_slice(b"e");
            }
        }
    }
}