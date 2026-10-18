use super::*;

fn contract(element: ElementShape) -> NestedValueContract {
    NestedValueContract {
        profile: PROFILE.to_string(),
        element,
        max_items: MAX_ITEMS,
        max_payload_bytes: MAX_PAYLOAD_BYTES,
        max_total_bytes: MAX_TOTAL_BYTES,
    }
}
fn hex(input: &str) -> Vec<u8> {
    input.as_bytes().chunks_exact(2).map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect()
}

// Test-only reference encoder. Frozen literal vectors below independently fix
// the expected wire format so a matching encoder/decoder mistake cannot pass.
fn bytes(payload: &[u8]) -> Vec<u8> {
    (payload.len() as u32).to_le_bytes().into_iter().chain(payload.iter().copied()).collect()
}
fn table(fields: &[Vec<u8>]) -> Vec<u8> {
    let header = 4 * (fields.len() + 1);
    let total = header + fields.iter().map(Vec::len).sum::<usize>();
    let mut encoded = (total as u32).to_le_bytes().to_vec();
    let mut offset = header;
    for field in fields {
        encoded.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += field.len();
    }
    for field in fields {
        encoded.extend_from_slice(field);
    }
    encoded
}
fn put_word(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn literal_bytes_vectors_bind_empty_and_mixed_payload_wire_format() {
    let c = contract(ElementShape::Bytes {});
    assert!(c.check(&[4, 0, 0, 0]).unwrap().elements().is_empty());
    let frozen = hex("1f0000001000000014000000190000000000000001000000ab020000000102");
    assert_eq!(table(&[bytes(&[]), bytes(&[0xab]), bytes(&[1, 2])]), frozen);
    let value = c.check(&frozen).unwrap();
    assert_eq!(value.bytes(), frozen);
    assert_eq!(value.elements().iter().map(NestedElementView::payload).collect::<Vec<_>>(), [&[][..], &[0xab], &[1, 2]]);
    assert!(value.elements().iter().all(|element| element.fixed_fields().is_empty()));
    // Views point into the validated bytes, not a copied payload buffer.
    assert_eq!(value.elements()[2].payload().as_ptr(), frozen[29..].as_ptr());
}

#[test]
fn literal_record_vector_binds_all_fixed_fields_and_payload() {
    let c = contract(ElementShape::Record { fixed_field_widths: vec![1, 2] });
    let frozen = hex("2100000008000000190000001000000011000000130000007f341202000000aabb");
    let record = table(&[vec![0x7f], vec![0x34, 0x12], bytes(&[0xaa, 0xbb])]);
    assert_eq!(table(&[record]), frozen);
    let value = c.check(&frozen).unwrap();
    let element = &value.elements()[0];
    assert_eq!(element.fixed_fields(), [&[0x7f][..], &[0x34, 0x12][..]]);
    assert_eq!(element.payload(), [0xaa, 0xbb]);
    assert!(c.check(&[4, 0, 0, 0]).unwrap().elements().is_empty());
}

#[test]
fn maximum_budgets_are_independent_and_checked_before_views_escape() {
    let c = contract(ElementShape::Bytes {});
    let maximum_count = table(&vec![bytes(&[]); MAX_ITEMS as usize]);
    assert_eq!(c.check(&maximum_count).unwrap().elements().len(), MAX_ITEMS as usize);
    let over_count = table(&vec![bytes(&[]); MAX_ITEMS as usize + 1]);
    assert_eq!(c.check(&over_count), Err(NestedValueError::ItemCountExceeded));

    let mut payloads = vec![bytes(&[0x55; MAX_PAYLOAD_BYTES as usize]); 15];
    payloads.push(bytes(&[0x66; 124]));
    let maximum_total = table(&payloads);
    assert_eq!(maximum_total.len(), MAX_TOTAL_BYTES as usize);
    assert_eq!(c.check(&maximum_total).unwrap().elements().len(), 16);
    payloads[15] = bytes(&[0x66; 125]);
    assert_eq!(c.check(&table(&payloads)), Err(NestedValueError::TotalBytesExceeded));

    let maximum_payload = table(&[bytes(&[0x77; MAX_PAYLOAD_BYTES as usize])]);
    assert_eq!(c.check(&maximum_payload).unwrap().elements()[0].payload().len(), MAX_PAYLOAD_BYTES as usize);
    assert_eq!(c.check(&table(&[bytes(&[0x77; MAX_PAYLOAD_BYTES as usize + 1])])), Err(NestedValueError::PayloadBytesExceeded));
}

#[test]
fn narrower_contracts_include_zero_budgets_without_changing_empty_encoding() {
    let mut c = contract(ElementShape::Bytes {});
    c.max_items = 0;
    c.max_payload_bytes = 0;
    c.max_total_bytes = 4;
    assert!(c.check(&[4, 0, 0, 0]).is_ok());
    assert_eq!(c.check(&[]), Err(NestedValueError::MalformedMolecule));
    assert_eq!(c.check(&[0, 0, 0, 0]), Err(NestedValueError::MalformedMolecule));
    c.max_total_bytes = 64;
    assert_eq!(c.check(&table(&[bytes(&[])])), Err(NestedValueError::ItemCountExceeded));
    c.max_items = 1;
    assert!(c.check(&table(&[bytes(&[])])).is_ok());
    assert_eq!(c.check(&table(&[bytes(&[0])])), Err(NestedValueError::PayloadBytesExceeded));
}

#[test]
fn fixed_field_width_and_field_count_are_exact_not_compatible_table_reads() {
    let c = contract(ElementShape::Record { fixed_field_widths: vec![1, 2] });
    for fields in [vec![vec![], vec![1, 2], bytes(&[])], vec![vec![0, 1], vec![1, 2], bytes(&[])], vec![vec![0], vec![1], bytes(&[])]]
    {
        assert_eq!(c.check(&table(&[table(&fields)])), Err(NestedValueError::FixedFieldWidthMismatch));
    }
    for fields in [vec![vec![0], bytes(&[])], vec![vec![0], vec![1, 2], bytes(&[]), vec![]]] {
        assert_eq!(c.check(&table(&[table(&fields)])), Err(NestedValueError::MalformedMolecule));
    }
    // An empty field is not an encoded empty Bytes value.
    assert_eq!(c.check(&table(&[table(&[vec![0], vec![1, 2], vec![]])])), Err(NestedValueError::MalformedMolecule));
}

#[test]
fn each_truncation_and_rebound_total_still_rejects() {
    let c = contract(ElementShape::Bytes {});
    let valid = table(&[bytes(&[1]), bytes(&[2, 3]), bytes(&[4, 5, 6])]);
    for end in 0..valid.len() {
        assert!(c.check(&valid[..end]).is_err(), "truncation at {end}");
        if end >= 8 {
            let mut truncated = valid[..end].to_vec();
            put_word(&mut truncated, 0, end as u32);
            assert!(c.check(&truncated).is_err(), "rebound total at {end}");
        }
    }
    let mut trailing = valid.clone();
    trailing.push(0);
    assert_eq!(c.check(&trailing), Err(NestedValueError::MalformedMolecule));
    let total = trailing.len() as u32;
    put_word(&mut trailing, 0, total);
    assert_eq!(c.check(&trailing), Err(NestedValueError::MalformedMolecule));
}

#[test]
fn offset_overflow_aliases_gaps_and_nested_size_mutations_reject() {
    let c = contract(ElementShape::Bytes {});
    let valid = table(&[bytes(&[1, 2]), bytes(&[3, 4])]);
    for (offset, value) in [(4, 0), (4, 4), (4, 11), (4, u32::MAX), (8, 0), (8, 12), (8, 17), (8, 19), (8, u32::MAX)] {
        let mut mutation = valid.clone();
        put_word(&mut mutation, offset, value);
        assert!(c.check(&mutation).is_err(), "offset {offset} changed to {value}");
    }
    for count in [0, 1, 3, u32::MAX] {
        let mut mutation = valid.clone();
        put_word(&mut mutation, 12, count);
        assert!(c.check(&mutation).is_err());
    }
    let c = contract(ElementShape::Record { fixed_field_widths: vec![1] });
    let valid = table(&[table(&[vec![0x99], bytes(&[7])])]);
    for (offset, value) in [(8, 0), (8, u32::MAX), (12, 8), (16, 12), (16, u32::MAX)] {
        let mut mutation = valid.clone();
        put_word(&mut mutation, offset, value);
        assert!(c.check(&mutation).is_err());
    }
}

#[test]
fn invalid_contracts_and_unknown_fields_do_not_enable_wider_codecs() {
    let original = contract(ElementShape::Bytes {});
    let mut contracts = Vec::new();
    let mut c = original.clone();
    c.profile.push_str("-unknown");
    contracts.push(c);
    let mut c = original.clone();
    c.max_items = MAX_ITEMS + 1;
    contracts.push(c);
    let mut c = original.clone();
    c.max_payload_bytes = MAX_PAYLOAD_BYTES + 1;
    contracts.push(c);
    for total in [0, 3, MAX_TOTAL_BYTES + 1, u32::MAX] {
        let mut c = original.clone();
        c.max_total_bytes = total;
        contracts.push(c);
    }
    for widths in [vec![], vec![0], vec![257], vec![1; MAX_FIXED_FIELDS + 1]] {
        contracts.push(contract(ElementShape::Record { fixed_field_widths: widths }));
    }
    for c in contracts {
        assert_eq!(c.check(&[4, 0, 0, 0]), Err(NestedValueError::InvalidContract));
    }
    let mut json = serde_json::to_value(original).unwrap();
    json["recursive"] = serde_json::json!(true);
    assert!(serde_json::from_value::<NestedValueContract>(json).is_err());
    assert!(serde_json::from_str::<ElementShape>(r#"{"kind":"bytes","extra":true}"#).is_err());
    assert!(serde_json::from_str::<ElementShape>(r#"{"kind":"recursive"}"#).is_err());
}

#[test]
fn maximum_record_shape_is_bounded_and_borrowed() {
    let c = contract(ElementShape::Record { fixed_field_widths: vec![MAX_FIXED_FIELD_BYTES; MAX_FIXED_FIELDS] });
    let mut fields = vec![vec![0xa5; MAX_FIXED_FIELD_BYTES as usize]; MAX_FIXED_FIELDS];
    fields.push(bytes(&[0x5a; MAX_PAYLOAD_BYTES as usize]));
    let encoded = table(&[table(&fields)]);
    let value = c.check(&encoded).unwrap();
    assert_eq!(value.elements()[0].fixed_fields().len(), MAX_FIXED_FIELDS);
    assert_eq!(value.elements()[0].payload(), [0x5a; MAX_PAYLOAD_BYTES as usize]);
}
