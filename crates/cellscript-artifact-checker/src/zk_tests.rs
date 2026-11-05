use crate::zk::*;

fn request() -> Request {
    Request {
        verification_key: [7; 32],
        proof: [9; PROOF_BYTES],
        statement: Statement {
            domain: [1; 32],
            action: [2; 32],
            script_hash: [3; 32],
            old_data_hash: [4; 32],
            new_data_hash: [5; 32],
            input_transaction_hash: [0xff; 32],
            input_output_index: u32::MAX,
            transaction_hash: [0x99; 32],
        },
    }
}

#[test]
fn exact_molecule_offsets_and_all_header_mutations() {
    let request = request();
    let bytes = request.encode();
    assert_eq!(Request::decode(&bytes, &[7; 32]), Ok(request));
    for index in 0..108 {
        let mut mutated = bytes;
        mutated[index] ^= 1;
        assert!(Request::decode(&mutated, &[7; 32]).is_err(), "header byte {index}");
    }
    for len in 0..REQUEST_BYTES {
        assert_eq!(Request::decode(&bytes[..len], &[7; 32]), Err(DecodeError::Length));
    }
    let mut long = bytes.to_vec();
    long.push(0);
    assert_eq!(Request::decode(&long, &[7; 32]), Err(DecodeError::Length));
    assert_eq!(Request::decode(&bytes, &[8; 32]), Err(DecodeError::VerificationKey));
}

#[test]
fn field_injection_preserves_every_bit_and_canonical_ranges() {
    let statement = request().statement;
    let encoded = statement.encode();
    let inputs = statement.public_inputs();
    assert_eq!(&inputs[..4], &15u32.to_le_bytes());
    for index in 0..12 {
        assert_eq!(&inputs[4 + index * 32..20 + index * 32], &encoded[index * 16..(index + 1) * 16]);
        assert!(inputs[20 + index * 32..36 + index * 32].iter().all(|byte| *byte == 0));
    }
    assert_eq!(&inputs[388..392], &u32::MAX.to_le_bytes());
    assert!(inputs[392..420].iter().all(|byte| *byte == 0));
    for index in 0..2 {
        assert_eq!(&inputs[420 + index * 32..436 + index * 32], &statement.transaction_hash[index * 16..(index + 1) * 16]);
        assert!(inputs[436 + index * 32..452 + index * 32].iter().all(|byte| *byte == 0));
    }
    for index in 0..STATEMENT_BYTES {
        let mut changed = encoded;
        changed[index] ^= 1;
        assert_ne!(Statement::decode(&changed).unwrap().public_inputs(), inputs);
    }
}
