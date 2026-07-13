use backend::core::bencode::BencodeValue;
use std::collections::BTreeMap;

// ============================================================================
// 基础解码测试 (已有)
// ============================================================================

#[test]
fn test_decode_integer() {
    let (value, n) = BencodeValue::decode(b"i42e").unwrap();
    assert_eq!(n, 4);
    assert_eq!(value, BencodeValue::Integer(42));
}

#[test]
fn test_decode_negative_integer() {
    let (value, _) = BencodeValue::decode(b"i-99e").unwrap();
    assert_eq!(value, BencodeValue::Integer(-99));
}

#[test]
fn test_decode_byte_string() {
    let (value, n) = BencodeValue::decode(b"4:spam").unwrap();
    assert_eq!(n, 6);
    assert_eq!(value, BencodeValue::ByteString(b"spam".to_vec()));
}

#[test]
fn test_decode_empty_byte_string() {
    let (value, _) = BencodeValue::decode(b"0:").unwrap();
    assert_eq!(value, BencodeValue::ByteString(vec![]));
}

#[test]
fn test_decode_list() {
    let (value, n) = BencodeValue::decode(b"l4:spami42ee").unwrap();
    assert_eq!(n, 12);
    assert_eq!(
        value,
        BencodeValue::List(vec![
            BencodeValue::ByteString(b"spam".to_vec()),
            BencodeValue::Integer(42),
        ])
    );
}

#[test]
fn test_decode_dict() {
    let (value, _) = BencodeValue::decode(b"d3:key5:valuee").unwrap();
    let mut expected = BTreeMap::new();
    expected.insert(b"key".to_vec(), BencodeValue::ByteString(b"value".to_vec()));
    assert_eq!(value, BencodeValue::Dict(expected));
}

#[test]
fn test_roundtrip_torrent_like() {
    let bencoded_bytes = b"d4:infod6:lengthi12345e4:name8:test.txte8:announce30:http://tracker.example.com/anne";
    let (decoded, n) = BencodeValue::decode(bencoded_bytes).unwrap();
    assert_eq!(n, bencoded_bytes.len());
    let re_encoded = decoded.encode();
    let (decoded2, _) = BencodeValue::decode(&re_encoded).unwrap();
    assert_eq!(decoded, decoded2);
}

#[test]
fn test_dict_get() {
    let (value, _) = BencodeValue::decode(b"d3:key5:value3:numi42ee").unwrap();
    assert_eq!(value.dict_get_str(b"key").unwrap(), "value");
    assert_eq!(value.dict_get_int(b"num").unwrap(), 42);
    assert!(value.dict_get(b"nonexistent").is_none());
}

// ============================================================================
// 1. 空容器测试 — 空列表与空字典
// ============================================================================

#[test]
fn test_decode_empty_list() {
    let (value, n) = BencodeValue::decode(b"le").unwrap();
    assert_eq!(n, 2);
    assert_eq!(value, BencodeValue::List(vec![]));
}

#[test]
fn test_decode_empty_dict() {
    let (value, n) = BencodeValue::decode(b"de").unwrap();
    assert_eq!(n, 2);
    assert_eq!(value, BencodeValue::Dict(BTreeMap::new()));
}

// ============================================================================
// 2. 零值整数与大整数边界
// ============================================================================

#[test]
fn test_decode_zero_integer() {
    let (value, _) = BencodeValue::decode(b"i0e").unwrap();
    assert_eq!(value, BencodeValue::Integer(0));
}

#[test]
fn test_decode_integer_i64_max() {
    let input = format!("i{}e", i64::MAX);
    let (value, _) = BencodeValue::decode(input.as_bytes()).unwrap();
    assert_eq!(value, BencodeValue::Integer(i64::MAX));
}

#[test]
fn test_decode_integer_i64_min() {
    let input = format!("i{}e", i64::MIN);
    let (value, _) = BencodeValue::decode(input.as_bytes()).unwrap();
    assert_eq!(value, BencodeValue::Integer(i64::MIN));
}

#[test]
fn test_roundtrip_integer_boundary() {
    // i64::MAX 与 i64::MIN 的 encode → decode 往返
    for &val in &[0i64, 1, -1, i64::MAX, i64::MIN] {
        let encoded = BencodeValue::Integer(val).encode();
        let (decoded, _) = BencodeValue::decode(&encoded).unwrap();
        assert_eq!(decoded, BencodeValue::Integer(val));
    }
}

// ============================================================================
// 3. 二进制字节串 — 包含不可打印字符、null 字节、高位字节
// ============================================================================

#[test]
fn test_decode_byte_string_with_null_bytes() {
    // 包含内嵌 \0 的字节串
    let data = b"\x00hello\x00world\x00".to_vec();
    let mut input = vec![];
    input.extend_from_slice(data.len().to_string().as_bytes());
    input.push(b':');
    input.extend_from_slice(&data);

    let (value, n) = BencodeValue::decode(&input).unwrap();
    assert_eq!(n, input.len());
    assert_eq!(value, BencodeValue::ByteString(data));
}

#[test]
fn test_decode_byte_string_binary_range() {
    // 覆盖 0x00–0xFF 的全部字节
    let data: Vec<u8> = (0u8..=255u8).collect();
    let mut input = vec![];
    input.extend_from_slice(data.len().to_string().as_bytes());
    input.push(b':');
    input.extend_from_slice(&data);

    let (value, n) = BencodeValue::decode(&input).unwrap();
    assert_eq!(n, input.len());
    assert_eq!(value, BencodeValue::ByteString(data));
}

#[test]
fn test_roundtrip_binary_byte_string() {
    let data = b"\x00\x01\x02\xfe\xffrandom binary \x89\xab\xcd".to_vec();
    let encoded = BencodeValue::ByteString(data.clone()).encode();
    let (decoded, _) = BencodeValue::decode(&encoded).unwrap();
    assert_eq!(decoded, BencodeValue::ByteString(data));
}

// ============================================================================
// 4. 深度嵌套结构
// ============================================================================

#[test]
fn test_decode_deeply_nested_lists() {
    // [[[[]]]] → 四层嵌套: l l l l e e e e
    let input = b"lllleeee";
    let (value, n) = BencodeValue::decode(input).unwrap();
    assert_eq!(n, input.len());

    let expected = BencodeValue::List(vec![
        BencodeValue::List(vec![
            BencodeValue::List(vec![
                BencodeValue::List(vec![]),
            ]),
        ]),
    ]);
    assert_eq!(value, expected);
}

#[test]
fn test_decode_list_of_dicts() {
    // [{"a": 1}, {"b": 2}]
    let input = b"ld1:ai1eed1:bi2eee";
    let (value, n) = BencodeValue::decode(input).unwrap();
    assert_eq!(n, input.len());

    let mut d1 = BTreeMap::new();
    d1.insert(b"a".to_vec(), BencodeValue::Integer(1));
    let mut d2 = BTreeMap::new();
    d2.insert(b"b".to_vec(), BencodeValue::Integer(2));

    assert_eq!(
        value,
        BencodeValue::List(vec![
            BencodeValue::Dict(d1),
            BencodeValue::Dict(d2),
        ])
    );
}

#[test]
fn test_decode_dict_with_list_values() {
    // {"fruits": ["apple", "banana"], "counts": 2}
    let input = b"d6:countsi2e6:fruitsl5:apple6:bananaee";
    let (value, n) = BencodeValue::decode(input).unwrap();
    assert_eq!(n, input.len());

    let mut expected = BTreeMap::new();
    expected.insert(
        b"counts".to_vec(),
        BencodeValue::Integer(2),
    );
    expected.insert(
        b"fruits".to_vec(),
        BencodeValue::List(vec![
            BencodeValue::ByteString(b"apple".to_vec()),
            BencodeValue::ByteString(b"banana".to_vec()),
        ]),
    );
    assert_eq!(value, BencodeValue::Dict(expected));
}

#[test]
fn test_roundtrip_complex_nested() {
    // 三层混合嵌套: {"a": [1, {"b": "c"}], "d": [[], "hello"]}
    let mut inner_dict = BTreeMap::new();
    inner_dict.insert(b"b".to_vec(), BencodeValue::ByteString(b"c".to_vec()));

    let mut root = BTreeMap::new();
    root.insert(
        b"a".to_vec(),
        BencodeValue::List(vec![
            BencodeValue::Integer(1),
            BencodeValue::Dict(inner_dict),
        ]),
    );
    root.insert(
        b"d".to_vec(),
        BencodeValue::List(vec![
            BencodeValue::List(vec![]),
            BencodeValue::ByteString(b"hello".to_vec()),
        ]),
    );

    let original = BencodeValue::Dict(root);
    let encoded = original.encode();
    let (decoded, n) = BencodeValue::decode(&encoded).unwrap();
    assert_eq!(n, encoded.len());
    assert_eq!(decoded, original);
}

// ============================================================================
// 5. 多键字典 / key 排序验证
// ============================================================================

#[test]
fn test_decode_dict_multiple_keys() {
    // {"z": 3, "a": 1, "m": 2} — key 按字节序排列
    let input = b"d1:ai1e1:mi2e1:zi3ee";
    let (value, _) = BencodeValue::decode(input).unwrap();

    let mut expected = BTreeMap::new();
    expected.insert(b"a".to_vec(), BencodeValue::Integer(1));
    expected.insert(b"m".to_vec(), BencodeValue::Integer(2));
    expected.insert(b"z".to_vec(), BencodeValue::Integer(3));
    assert_eq!(value, BencodeValue::Dict(expected));

    // 编码后 key 必须按字典序输出
    let encoded = String::from_utf8(value.encode()).unwrap();
    assert_eq!(encoded, "d1:ai1e1:mi2e1:zi3ee");
}

// ============================================================================
// 6. 超长字节串与大容量往返
// ============================================================================

#[test]
fn test_decode_large_byte_string() {
    // 10 KiB 的重复数据
    let data = vec![b'A'; 10240];
    let mut input = vec![];
    input.extend_from_slice(b"10240:");
    input.extend_from_slice(&data);

    let (value, n) = BencodeValue::decode(&input).unwrap();
    assert_eq!(n, input.len());
    assert_eq!(value, BencodeValue::ByteString(data));
}

#[test]
fn test_roundtrip_large_torrent_like_dict() {
    // 构造一个包含多种类型的较大 dict 做往返
    let mut info = BTreeMap::new();
    info.insert(b"name".to_vec(), BencodeValue::ByteString(b"big_file.iso".to_vec()));
    info.insert(b"piece length".to_vec(), BencodeValue::Integer(1 << 20));
    info.insert(b"length".to_vec(), BencodeValue::Integer(4_294_967_296));
    // 构造一个假的 pieces (20字节 × 3 = 60 字节)
    let pieces: Vec<u8> = (0..60).map(|i| (i % 256) as u8).collect();
    info.insert(b"pieces".to_vec(), BencodeValue::ByteString(pieces));

    let mut torrent = BTreeMap::new();
    torrent.insert(b"announce".to_vec(), BencodeValue::ByteString(b"http://tracker.example.com:6969/announce".to_vec()));
    torrent.insert(b"info".to_vec(), BencodeValue::Dict(info));
    torrent.insert(b"creation date".to_vec(), BencodeValue::Integer(1713398400));
    torrent.insert(b"comment".to_vec(), BencodeValue::ByteString(b"robustness test torrent".to_vec()));

    let original = BencodeValue::Dict(torrent);
    let encoded = original.encode();
    let (decoded, n) = BencodeValue::decode(&encoded).unwrap();
    assert_eq!(n, encoded.len());
    assert_eq!(decoded, original);

    // 验证个别字段可以正确取出
    assert_eq!(decoded.dict_get_str(b"comment").unwrap(), "robustness test torrent");
    let info_dict = decoded.dict_get(b"info").unwrap();
    assert_eq!(info_dict.dict_get_int(b"length").unwrap(), 4_294_967_296);
    assert_eq!(info_dict.dict_get_int(b"piece length").unwrap(), 1 << 20);
    assert_eq!(info_dict.dict_get_bytes(b"pieces").unwrap().len(), 60);
}

// ============================================================================
// 7. 错误 / 鲁棒性: 非法输入应返回 Err（而非 panic）
// ============================================================================

#[test]
fn test_error_truncated_integer_no_e() {
    // i42 — 缺少结尾 e
    let result = BencodeValue::decode(b"i42");
    assert!(result.is_err());
}

#[test]
fn test_error_truncated_integer_eof() {
    // i — 仅有一个前缀
    let result = BencodeValue::decode(b"i");
    assert!(result.is_err());
}

#[test]
fn test_error_truncated_list_no_e() {
    // li42e — 列表缺少结尾 e（退化为单整数）
    let result = BencodeValue::decode(b"li42e");
    assert!(result.is_err());
}

#[test]
fn test_error_truncated_dict_no_e() {
    // d3:keyi1e — 字典缺少结尾 e
    let result = BencodeValue::decode(b"d3:keyi1e");
    assert!(result.is_err());
}

#[test]
fn test_error_leading_zero_in_length_prefix() {
    // 0 开头的长度前缀 "04:spam" 在严格 Bencode 中允许，这里检查不会 panic
    let result = BencodeValue::decode(b"04:spam");
    // 应该能解析（长度 04 = 4），但也可以拒绝；这里只保证不 panic
    match result {
        Ok((value, _)) => {
            // 如果解析器允许前导零，则正确解析
            assert_eq!(value, BencodeValue::ByteString(b"spam".to_vec()));
        }
        Err(_) => {
            // 如果解析器拒绝前导零，也是合理的
        }
    }
}

#[test]
fn test_error_unexpected_start_byte() {
    // 以非法字符开头（非 i / l / d / 数字）
    let result = BencodeValue::decode(b"x");
    assert!(result.is_err());
}

#[test]
fn test_error_dict_key_not_byte_string() {
    // d 后面跟非字节串 key: d i1e3:fooe
    let result = BencodeValue::decode(b"di1e3:fooe");
    assert!(result.is_err());
}

#[test]
fn test_error_byte_string_length_exceeds_input() {
    // 10:short — 声称 10 字节但只有 5 字节
    let result = BencodeValue::decode(b"10:short");
    assert!(result.is_err());
}

#[test]
fn test_error_non_digit_in_length_prefix() {
    // abc:data — 长度前缀不是数字
    let result = BencodeValue::decode(b"abc:data");
    assert!(result.is_err());
}

#[test]
fn test_error_empty_input() {
    let result = BencodeValue::decode(b"");
    assert!(result.is_err());
}

#[test]
fn test_error_negative_zero() {
    // i-0e — 负零，多数解析器拒绝
    let result = BencodeValue::decode(b"i-0e");
    // 当前实现可能允许也可能拒绝；只保证不 panic
    if let Ok((value, _)) = result {
        assert_eq!(value, BencodeValue::Integer(0));
    }
}

// ============================================================================
// 8. as_str / as_int / as_bytes 在错误类型上的行为
// ============================================================================

#[test]
fn test_as_str_on_non_bytes() {
    let val = BencodeValue::Integer(42);
    assert!(val.as_str().is_err());
}

#[test]
fn test_as_int_on_non_integer() {
    let val = BencodeValue::ByteString(b"hello".to_vec());
    assert!(val.as_int().is_none());
}

#[test]
fn test_as_bytes_on_non_bytes() {
    let val = BencodeValue::Integer(42);
    assert!(val.as_bytes().is_none());
}

#[test]
fn test_dict_get_on_non_dict() {
    let val = BencodeValue::Integer(42);
    assert!(val.dict_get(b"any").is_none());
    assert!(val.dict_get_str(b"any").is_none());
    assert!(val.dict_get_int(b"any").is_none());
    assert!(val.dict_get_bytes(b"any").is_none());
}

// ============================================================================
// 9. 单元素嵌套往返测试
// ============================================================================

#[test]
fn test_roundtrip_single_element_each_type() {
    // 每种基本类型单独做 roundtrip
    let test_cases: Vec<BencodeValue> = vec![
        BencodeValue::Integer(0),
        BencodeValue::Integer(-1),
        BencodeValue::Integer(1024),
        BencodeValue::ByteString(vec![]),
        BencodeValue::ByteString(b"simple".to_vec()),
        BencodeValue::ByteString(vec![0, 255, 128, 64]),
        BencodeValue::List(vec![]),
        BencodeValue::List(vec![BencodeValue::Integer(1), BencodeValue::Integer(2)]),
        BencodeValue::Dict(BTreeMap::new()),
    ];

    for original in &test_cases {
        let encoded = original.encode();
        let (decoded, n) = BencodeValue::decode(&encoded).unwrap();
        assert_eq!(n, encoded.len(), "roundtrip length mismatch for {:?}", original);
        assert_eq!(&decoded, original, "roundtrip value mismatch for {:?}", original);
    }
}

// ============================================================================
// 10. 部分解析测试 — 确认返回的字节数准确
// ============================================================================

#[test]
fn test_partial_decode_with_trailing_data() {
    // i42eXtraData — 解析整数后应报告消耗 4 字节
    let (value, n) = BencodeValue::decode(b"i42eXtraData").unwrap();
    assert_eq!(n, 4);
    assert_eq!(value, BencodeValue::Integer(42));
}

#[test]
fn test_partial_decode_string_with_trailing_data() {
    // 4:spamMore — 只应消费 6 字节
    let (value, n) = BencodeValue::decode(b"4:spamMore").unwrap();
    assert_eq!(n, 6);
    assert_eq!(value, BencodeValue::ByteString(b"spam".to_vec()));
}