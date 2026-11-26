const VEC_BUILTIN_PROGRAM: &str = r#"
module test;

action pack(witness bytes: [u8; 3]) -> u64 {

        let mut data = Vec::new();
        data.push(bytes[0]);
        data.extend_from_slice(bytes);
        return data.len();
}
"#;

const FIXED_WIDTH_VEC_CREATE_PROGRAM: &str = r#"
module test;

resource Group {
    members: Vec<Address>,
    anchors: Vec<Hash>,
}

action create_group(witness owner: Address, witness seed: Hash) -> Group {

        let mut members = Vec::new();
        members.push(owner);
        let mut anchors = Vec::new();
        anchors.push(seed);
        return create Group {
            members: members,
            anchors: anchors,
        };
}
"#;

const STACK_VEC_RUNTIME_PROGRAM: &str = r#"
module test;

action stack_vec_sum() -> u64 {

        let mut values = Vec::new();
        values.push(7);
        values.push(9);
        return values.len() + values[1];
}
"#;

const STACK_VEC_WITH_CAPACITY_PROGRAM: &str = r#"
module test;

action stack_vec_with_capacity_sum() -> u64 {

        let mut values = Vec::with_capacity(2);
        values.push(7);
        values.push(9);
        return values.len() + values[1];
}
"#;

const BOUNDED_VEC_LITERAL_PROGRAM: &str = r#"
module test;

action bounded_vec_literal_sum() -> u64 {

        let mut values: Vec<u64> = [7, 9];
        return values.len() + values[1];
}
"#;

const EMPTY_VEC_LITERAL_PROGRAM: &str = r#"
module test;

action empty_vec_literal_capacity() -> u64 {

        let values: Vec<Address> = [];
        return values.capacity();
}
"#;

const CREATE_VEC_LITERAL_FIELD_PROGRAM: &str = r#"
module test;

resource Group has store {
    members: Vec<Address>,
    labels: Vec<u8>,
}

action create_group(witness owner: Address) -> Group {

        return create Group {
            members: [owner],
            labels: [],
        };
}
"#;

const STACK_VEC_CAPACITY_RUNTIME_PROGRAM: &str = r#"
module test;

action stack_vec_capacity_value() -> u64 {

        let mut values = Vec::with_capacity(2);
        values.push(7);
        return values.capacity();
}
"#;

const FIXED_BYTE_STACK_VEC_CAPACITY_PROGRAM: &str = r#"
module test;

action stack_vec_address_capacity(witness owner: Address) -> u64 {

        let mut owners = Vec::with_capacity(2);
        owners.push(owner);
        return owners.capacity();
}
"#;

const STACK_VEC_REMOVE_RUNTIME_PROGRAM: &str = r#"
module test;

action stack_vec_remove_middle() -> u64 {

        let mut values = Vec::new();
        values.push(7);
        values.push(9);
        values.push(11);
        let removed = values.remove(1);
        return removed + values.len() + values[1];
}
"#;

const FIXED_BYTE_STACK_VEC_REMOVE_PROGRAM: &str = r#"
module test;

action stack_vec_address_remove(witness first: Address, witness second: Address) -> bool {

        let mut owners = Vec::new();
        owners.push(first);
        owners.push(second);
        let removed = owners.remove(0);
        if removed == first {
            return owners[0] == second;
        }
        return false;
}
"#;

const STACK_VEC_POP_RUNTIME_PROGRAM: &str = r#"
module test;

action stack_vec_pop_last() -> u64 {

        let mut values = Vec::new();
        values.push(7);
        values.push(11);
        let popped = values.pop();
        return popped + values.len() + values[0];
}
"#;

const FIXED_BYTE_STACK_VEC_POP_PROGRAM: &str = r#"
module test;

action stack_vec_address_pop(witness owner: Address) -> bool {

        let mut owners = Vec::new();
        owners.push(owner);
        let popped = owners.pop();
        return popped == owner;
}
"#;

const STACK_VEC_INSERT_RUNTIME_PROGRAM: &str = r#"
module test;

action stack_vec_insert_middle() -> u64 {

        let mut values = Vec::new();
        values.push(7);
        values.push(11);
        values.insert(1, 9);
        return values.len() + values[1] + values[2];
}
"#;

const FIXED_BYTE_STACK_VEC_INSERT_PROGRAM: &str = r#"
module test;

action stack_vec_address_insert(witness owner: Address, witness candidate: Address) -> bool {

        let mut owners = Vec::new();
        owners.push(owner);
        owners.insert(0, candidate);
        if owners[0] == candidate {
            return owners[1] == owner;
        }
        return false;
}
"#;

const STACK_VEC_SET_RUNTIME_PROGRAM: &str = r#"
module test;

action stack_vec_set_middle() -> u64 {

        let mut values = Vec::new();
        values.push(7);
        values.push(11);
        values.set(1, 9);
        return values.len() + values[0] + values[1];
}
"#;

const FIXED_BYTE_STACK_VEC_SET_PROGRAM: &str = r#"
module test;

action stack_vec_address_set(witness owner: Address, witness candidate: Address) -> bool {

        let mut owners = Vec::new();
        owners.push(owner);
        owners.push(owner);
        owners.set(1, candidate);
        if owners[0] == owner {
            return owners[1] == candidate;
        }
        return false;
}
"#;

const STACK_VEC_REVERSE_RUNTIME_PROGRAM: &str = r#"
module test;

action stack_vec_reverse_sum() -> u64 {

        let mut values = Vec::new();
        values.push(7);
        values.push(9);
        values.push(11);
        values.reverse();
        return values[0] + values[2];
}
"#;

const FIXED_BYTE_STACK_VEC_REVERSE_PROGRAM: &str = r#"
module test;

action stack_vec_address_reverse(witness first: Address, witness second: Address) -> bool {

        let mut owners = Vec::new();
        owners.push(first);
        owners.push(second);
        owners.reverse();
        if owners[0] == second {
            return owners[1] == first;
        }
        return false;
}
"#;

const STACK_VEC_SWAP_RUNTIME_PROGRAM: &str = r#"
module test;

action stack_vec_swap_sum() -> u64 {

        let mut values = Vec::new();
        values.push(7);
        values.push(9);
        values.push(11);
        values.swap(0, 2);
        return values[0] + values[2];
}
"#;

const FIXED_BYTE_STACK_VEC_SWAP_PROGRAM: &str = r#"
module test;

action stack_vec_address_swap(witness first: Address, witness second: Address) -> bool {

        let mut owners = Vec::new();
        owners.push(first);
        owners.push(second);
        owners.swap(0, 1);
        if owners[0] == second {
            return owners[1] == first;
        }
        return false;
}
"#;

const STACK_VEC_FIRST_LAST_RUNTIME_PROGRAM: &str = r#"
module test;

action stack_vec_first_last_sum() -> u64 {

        let mut values = Vec::new();
        values.push(7);
        values.push(9);
        values.push(11);
        return values.first() + values.last();
}
"#;

const FIXED_BYTE_STACK_VEC_FIRST_LAST_PROGRAM: &str = r#"
module test;

action stack_vec_address_first_last(witness first: Address, witness second: Address) -> bool {

        let mut owners = Vec::new();
        owners.push(first);
        owners.push(second);
        if owners.first() == first {
            return owners.last() == second;
        }
        return false;
}
"#;

const STACK_VEC_TRUNCATE_RUNTIME_PROGRAM: &str = r#"
module test;

action stack_vec_truncate_len() -> u64 {

        let mut values = Vec::new();
        values.push(7);
        values.push(9);
        values.push(11);
        values.truncate(2);
        return values.len() + values[1];
}
"#;

const FIXED_BYTE_STACK_VEC_TRUNCATE_PROGRAM: &str = r#"
module test;

action stack_vec_address_truncate(witness first: Address, witness second: Address) -> bool {

        let mut owners = Vec::new();
        owners.push(first);
        owners.push(second);
        owners.truncate(1);
        if owners.len() == 1 {
            return owners[0] == first;
        }
        return false;
}
"#;

const FIXED_BYTE_STACK_VEC_RUNTIME_PROGRAM: &str = r#"
module test;

action stack_vec_address_roundtrip(witness owner: Address) -> bool {

        let mut owners = Vec::new();
        owners.push(owner);
        return owners[0] == owner;
}
"#;

const STACK_VEC_EXTEND_RUNTIME_PROGRAM: &str = r#"
module test;

action stack_vec_extend_len(witness seed: [u8; 3]) -> u64 {

        let mut bytes = Vec::new();
        bytes.extend_from_slice(seed);
        return bytes.len();
}
"#;

const STACK_VEC_CLEAR_IS_EMPTY_PROGRAM: &str = r#"
module test;

action stack_vec_clear_len() -> u64 {

        let mut values = Vec::new();
        values.push(7);
        values.clear();
        if values.is_empty() {
            return values.len();
        }
        return 99;
}
"#;

const STACK_VEC_CONTAINS_RUNTIME_PROGRAM: &str = r#"
module test;

action stack_vec_contains_hit() -> u64 {

        let mut values = Vec::new();
        values.push(7);
        values.push(9);
        if values.contains(9) {
            return 1;
        }
        return 0;
}
"#;

const FIXED_BYTE_STACK_VEC_CONTAINS_PROGRAM: &str = r#"
module test;

action stack_vec_address_contains(witness owner: Address, witness candidate: Address) -> bool {

        let mut owners = Vec::new();
        owners.push(owner);
        return owners.contains(candidate);
}
"#;

const FIXED_SCHEMA_STACK_VEC_PROGRAM: &str = r#"
module test;

struct Snapshot {
    owner: Address,
    amount: u64,
}

action stack_vec_snapshot_roundtrip(witness first: Snapshot, witness second: Snapshot, witness third: Snapshot) -> bool {

        let mut snapshots = Vec::with_capacity(3);
        snapshots.push(first);
        snapshots.insert(0, second);
        snapshots.swap(0, 1);
        snapshots.set(1, third);
        snapshots.reverse();
        let removed = snapshots.remove(0);
        snapshots.push(removed);
        snapshots.truncate(2);

        if snapshots.capacity() == 6 {
            if snapshots.contains(third) {
                let loaded = snapshots.pop();
                if loaded.owner == third.owner {
                    return loaded.amount == third.amount;
                }
            }
        }

        return false;

}
action stack_vec_snapshot_first_last(witness first: Snapshot, witness second: Snapshot) -> bool {

        let mut snapshots = Vec::new();
        snapshots.push(first);
        snapshots.push(second);

        if snapshots.first().owner == first.owner {
            return snapshots.last().amount == second.amount;
        }

        return false;

}
action stack_vec_snapshot_clear(witness snapshot: Snapshot) -> u64 {

        let mut snapshots = Vec::new();
        snapshots.push(snapshot);
        snapshots.clear();

        if snapshots.is_empty() {
            return snapshots.len();
        }

        return 99;

}
action stack_vec_snapshot_pop(witness snapshot: Snapshot) -> bool {

        let mut snapshots = Vec::new();
        snapshots.push(snapshot);

        if snapshots.contains(snapshot) {
            let loaded = snapshots.pop();
            if loaded.owner == snapshot.owner {
                return loaded.amount == snapshot.amount;
            }
        }

        return false;
}
"#;

const CELL_BACKED_VEC_PROGRAM: &str = r#"
module test;

resource NFT {
    token_id: u64,
    owner: Address,
}

action batch_mint(witness owner: Address) -> Vec<NFT> {

        let mut nfts = Vec::new();
        let nft = create NFT {
            token_id: 1,
            owner: owner,
        };
        nfts.push(nft);
        return nfts;
}
"#;
