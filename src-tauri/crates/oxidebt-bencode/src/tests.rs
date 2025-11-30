use super::*;
use bytes::Bytes;
use std::collections::BTreeMap;

#[test]
fn test_decode_integer() {
    assert_eq!(decode(b"i42e").unwrap(), Value::Integer(42));
    assert_eq!(decode(b"i0e").unwrap(), Value::Integer(0));
    assert_eq!(decode(b"i-42e").unwrap(), Value::Integer(-42));
    assert_eq!(
        decode(b"i9223372036854775807e").unwrap(),
        Value::Integer(i64::MAX)
    );
}

#[test]
fn test_decode_integer_errors() {
    assert!(matches!(decode(b"i01e"), Err(DecodeError::LeadingZeros)));
    assert!(matches!(decode(b"i-0e"), Err(DecodeError::NegativeZero)));
    assert!(matches!(decode(b"ie"), Err(DecodeError::InvalidInteger(_))));
    assert!(matches!(decode(b"i"), Err(DecodeError::UnexpectedEof)));
}

#[test]
fn test_decode_bytes() {
    assert_eq!(
        decode(b"4:spam").unwrap(),
        Value::Bytes(Bytes::from_static(b"spam"))
    );
    assert_eq!(
        decode(b"0:").unwrap(),
        Value::Bytes(Bytes::from_static(b""))
    );
    assert_eq!(
        decode(b"5:hello").unwrap(),
        Value::Bytes(Bytes::from_static(b"hello"))
    );
}

#[test]
fn test_decode_bytes_binary() {
    let data = b"4:\x00\x01\x02\x03";
    assert_eq!(
        decode(data).unwrap(),
        Value::Bytes(Bytes::from_static(b"\x00\x01\x02\x03"))
    );
}

#[test]
fn test_decode_list() {
    assert_eq!(decode(b"le").unwrap(), Value::List(vec![]));
    assert_eq!(
        decode(b"li1ei2ei3ee").unwrap(),
        Value::List(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3)
        ])
    );
    assert_eq!(
        decode(b"l4:spami42ee").unwrap(),
        Value::List(vec![
            Value::Bytes(Bytes::from_static(b"spam")),
            Value::Integer(42)
        ])
    );
}

#[test]
fn test_decode_nested_list() {
    assert_eq!(
        decode(b"lli1ei2eeli3ei4eee").unwrap(),
        Value::List(vec![
            Value::List(vec![Value::Integer(1), Value::Integer(2)]),
            Value::List(vec![Value::Integer(3), Value::Integer(4)])
        ])
    );
}

#[test]
fn test_decode_dict() {
    assert_eq!(decode(b"de").unwrap(), Value::Dict(BTreeMap::new()));

    let mut expected = BTreeMap::new();
    expected.insert(Bytes::from_static(b"cow"), Value::string("moo"));
    expected.insert(Bytes::from_static(b"spam"), Value::string("eggs"));
    assert_eq!(
        decode(b"d3:cow3:moo4:spam4:eggse").unwrap(),
        Value::Dict(expected)
    );
}

#[test]
fn test_decode_dict_sorted() {
    assert!(matches!(
        decode(b"d4:spam4:eggs3:cow3:mooe"),
        Err(DecodeError::UnsortedKeys)
    ));
}

#[test]
fn test_decode_nested_dict() {
    let data = b"d4:infod4:name4:teste5:valuei42ee";
    let result = decode(data).unwrap();

    let dict = result.as_dict().unwrap();
    let info = dict.get(&Bytes::from_static(b"info")).unwrap();
    let info_dict = info.as_dict().unwrap();
    assert_eq!(
        info_dict.get(&Bytes::from_static(b"name")).unwrap(),
        &Value::string("test")
    );
}

#[test]
fn test_encode_integer() {
    assert_eq!(encode(&Value::Integer(42)).unwrap(), b"i42e");
    assert_eq!(encode(&Value::Integer(0)).unwrap(), b"i0e");
    assert_eq!(encode(&Value::Integer(-42)).unwrap(), b"i-42e");
}

#[test]
fn test_encode_bytes() {
    assert_eq!(encode(&Value::string("spam")).unwrap(), b"4:spam");
    assert_eq!(encode(&Value::string("")).unwrap(), b"0:");
}

#[test]
fn test_encode_list() {
    assert_eq!(encode(&Value::List(vec![])).unwrap(), b"le");
    assert_eq!(
        encode(&Value::List(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3)
        ]))
        .unwrap(),
        b"li1ei2ei3ee"
    );
}

#[test]
fn test_encode_dict() {
    let mut dict = BTreeMap::new();
    dict.insert(Bytes::from_static(b"cow"), Value::string("moo"));
    dict.insert(Bytes::from_static(b"spam"), Value::string("eggs"));

    let encoded = encode(&Value::Dict(dict)).unwrap();
    assert_eq!(encoded, b"d3:cow3:moo4:spam4:eggse");
}

#[test]
fn test_roundtrip() {
    let mut info = BTreeMap::new();
    info.insert(Bytes::from_static(b"length"), Value::Integer(12345));
    info.insert(Bytes::from_static(b"name"), Value::string("test.txt"));
    info.insert(Bytes::from_static(b"piece length"), Value::Integer(262144));
    info.insert(
        Bytes::from_static(b"pieces"),
        Value::Bytes(Bytes::from_static(&[0u8; 20])),
    );

    let mut torrent = BTreeMap::new();
    torrent.insert(
        Bytes::from_static(b"announce"),
        Value::string("http://tracker.example.com/announce"),
    );
    torrent.insert(Bytes::from_static(b"info"), Value::Dict(info));

    let original = Value::Dict(torrent);
    let encoded = encode(&original).unwrap();
    let decoded = decode(&encoded).unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_trailing_data() {
    assert!(matches!(
        decode(b"i42eextra"),
        Err(DecodeError::TrailingData)
    ));
}

#[test]
fn test_value_accessors() {
    let dict = decode(b"d3:foo3:bar3:numi42ee").unwrap();

    assert_eq!(dict.get_str("foo").unwrap().as_str(), Some("bar"));
    assert_eq!(dict.get_str("num").unwrap().as_integer(), Some(42));
    assert!(dict.get_str("missing").is_none());
}

#[test]
fn stress_test_large_integers() {
    let test_values = [
        i64::MIN,
        i64::MIN + 1,
        -1_000_000_000_000i64,
        -1,
        0,
        1,
        1_000_000_000_000i64,
        i64::MAX - 1,
        i64::MAX,
    ];

    for &val in &test_values {
        let encoded = encode(&Value::Integer(val)).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, Value::Integer(val));
    }
}

#[test]
fn stress_test_large_byte_strings() {
    let sizes = [0, 1, 10, 100, 1000, 10_000, 100_000, 1_000_000];

    for size in sizes {
        let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        let value = Value::Bytes(Bytes::from(data.clone()));
        let encoded = encode(&value).unwrap();
        let decoded = decode(&encoded).unwrap();

        match decoded {
            Value::Bytes(b) => assert_eq!(b.as_ref(), data.as_slice()),
            _ => panic!("Expected bytes"),
        }
    }
}

#[test]
fn stress_test_deeply_nested_lists() {
    let mut value = Value::Integer(42);
    for _ in 0..100 {
        value = Value::List(vec![value]);
    }

    let encoded = encode(&value).unwrap();
    let decoded = decode(&encoded).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn stress_test_deeply_nested_dicts() {
    let mut value = Value::Integer(42);
    for i in 0..100 {
        let mut dict = BTreeMap::new();
        let key = format!("level{}", i);
        dict.insert(Bytes::from(key), value);
        value = Value::Dict(dict);
    }

    let encoded = encode(&value).unwrap();
    let decoded = decode(&encoded).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn stress_test_wide_list() {
    let elements: Vec<Value> = (0..10_000).map(Value::Integer).collect();
    let value = Value::List(elements);

    let encoded = encode(&value).unwrap();
    let decoded = decode(&encoded).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn stress_test_wide_dict() {
    let mut dict = BTreeMap::new();
    for i in 0..10_000 {
        let key = format!("key_{:05}", i);
        dict.insert(Bytes::from(key), Value::Integer(i));
    }
    let value = Value::Dict(dict);

    let encoded = encode(&value).unwrap();
    let decoded = decode(&encoded).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn stress_test_mixed_complex_structure() {
    let mut inner_dict = BTreeMap::new();
    inner_dict.insert(
        Bytes::from_static(b"data"),
        Value::Bytes(Bytes::from(vec![0u8; 1000])),
    );
    inner_dict.insert(Bytes::from_static(b"count"), Value::Integer(999));

    let list: Vec<Value> = (0..100)
        .map(|i| {
            let mut d = BTreeMap::new();
            d.insert(Bytes::from(format!("item{}", i)), Value::Integer(i));
            d.insert(
                Bytes::from_static(b"nested"),
                Value::Dict(inner_dict.clone()),
            );
            Value::Dict(d)
        })
        .collect();

    let mut root = BTreeMap::new();
    root.insert(Bytes::from_static(b"items"), Value::List(list));
    root.insert(Bytes::from_static(b"version"), Value::Integer(1));
    let value = Value::Dict(root);

    let encoded = encode(&value).unwrap();
    let decoded = decode(&encoded).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn stress_test_binary_data_preservation() {
    let all_bytes: Vec<u8> = (0..=255).collect();
    let value = Value::Bytes(Bytes::from(all_bytes.clone()));

    let encoded = encode(&value).unwrap();
    let decoded = decode(&encoded).unwrap();

    match decoded {
        Value::Bytes(b) => assert_eq!(b.as_ref(), all_bytes.as_slice()),
        _ => panic!("Expected bytes"),
    }
}

#[test]
fn stress_test_rapid_encode_decode_cycles() {
    let mut dict = BTreeMap::new();
    dict.insert(Bytes::from_static(b"name"), Value::string("test"));
    dict.insert(Bytes::from_static(b"value"), Value::Integer(42));
    let value = Value::Dict(dict);

    for _ in 0..10_000 {
        let encoded = encode(&value).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, value);
    }
}

#[test]
fn stress_test_empty_structures() {
    let cases = [
        Value::List(vec![]),
        Value::Dict(BTreeMap::new()),
        Value::Bytes(Bytes::new()),
        Value::List(vec![Value::List(vec![]), Value::Dict(BTreeMap::new())]),
    ];

    for value in cases {
        let encoded = encode(&value).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, value);
    }
}

#[test]
fn stress_test_torrent_like_structure() {
    let piece_hashes: Vec<u8> = (0..1000)
        .flat_map(|i| {
            let mut hash = [0u8; 20];
            for (j, byte) in hash.iter_mut().enumerate() {
                *byte = ((i + j) % 256) as u8;
            }
            hash.into_iter()
        })
        .collect();

    let mut info = BTreeMap::new();
    info.insert(Bytes::from_static(b"length"), Value::Integer(1_000_000_000));
    info.insert(
        Bytes::from_static(b"name"),
        Value::string("ubuntu-22.04.iso"),
    );
    info.insert(Bytes::from_static(b"piece length"), Value::Integer(262144));
    info.insert(
        Bytes::from_static(b"pieces"),
        Value::Bytes(Bytes::from(piece_hashes)),
    );
    info.insert(Bytes::from_static(b"private"), Value::Integer(0));

    let announce_list = Value::List(vec![
        Value::List(vec![Value::string(
            "udp://tracker1.example.com:6969/announce",
        )]),
        Value::List(vec![Value::string(
            "udp://tracker2.example.com:6969/announce",
        )]),
        Value::List(vec![Value::string("http://tracker3.example.com/announce")]),
    ]);

    let mut torrent = BTreeMap::new();
    torrent.insert(
        Bytes::from_static(b"announce"),
        Value::string("udp://tracker.example.com:6969/announce"),
    );
    torrent.insert(Bytes::from_static(b"announce-list"), announce_list);
    torrent.insert(
        Bytes::from_static(b"comment"),
        Value::string("Ubuntu 22.04 LTS Desktop"),
    );
    torrent.insert(
        Bytes::from_static(b"created by"),
        Value::string("qBittorrent v4.5.0"),
    );
    torrent.insert(
        Bytes::from_static(b"creation date"),
        Value::Integer(1700000000),
    );
    torrent.insert(Bytes::from_static(b"info"), Value::Dict(info));

    let value = Value::Dict(torrent);

    let encoded = encode(&value).unwrap();
    let decoded = decode(&encoded).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn stress_test_multifile_torrent_structure() {
    let files: Vec<Value> = (0..100)
        .map(|i| {
            let path = Value::List(vec![
                Value::Bytes(Bytes::from(format!("folder{}", i / 10))),
                Value::Bytes(Bytes::from(format!("file{}.txt", i))),
            ]);
            let mut file = BTreeMap::new();
            file.insert(
                Bytes::from_static(b"length"),
                Value::Integer((i + 1) * 1000),
            );
            file.insert(Bytes::from_static(b"path"), path);
            Value::Dict(file)
        })
        .collect();

    let mut info = BTreeMap::new();
    info.insert(Bytes::from_static(b"files"), Value::List(files));
    info.insert(
        Bytes::from_static(b"name"),
        Value::string("multifile_torrent"),
    );
    info.insert(Bytes::from_static(b"piece length"), Value::Integer(16384));
    info.insert(
        Bytes::from_static(b"pieces"),
        Value::Bytes(Bytes::from(vec![0u8; 200])),
    );

    let mut torrent = BTreeMap::new();
    torrent.insert(
        Bytes::from_static(b"announce"),
        Value::string("http://tracker.example.com/announce"),
    );
    torrent.insert(Bytes::from_static(b"info"), Value::Dict(info));

    let value = Value::Dict(torrent);

    let encoded = encode(&value).unwrap();
    let decoded = decode(&encoded).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn stress_test_unicode_strings() {
    let unicode_strings = [
        "Hello, 世界!",
        "Привет мир",
        "🎉🎊🎈",
        "مرحبا بالعالم",
        "日本語テスト",
        "Ελληνικά",
        "한국어 테스트",
    ];

    for s in unicode_strings {
        let value = Value::string(s);
        let encoded = encode(&value).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.as_str(), Some(s));
    }
}

#[test]
fn stress_test_error_recovery() {
    let malformed_inputs: &[&[u8]] = &[
        b"i42",
        b"5:abc",
        b"d3:keye",
        b"l",
        b"d",
        b"iABCe",
        b"-5:hello",
        b"i00e",
        b"d4:spam3:bbb3:aaa3:ccce",
    ];

    for input in malformed_inputs {
        assert!(decode(input).is_err(), "Should fail for: {:?}", input);
    }
}
