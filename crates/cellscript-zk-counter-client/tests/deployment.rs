use cellscript_zk_counter_client::{parent, Deployment};
use cellscript_zk_private_counter as counter;

#[test]
fn substituted_deployment_bytes_name_the_mismatched_commitment() {
    let deployment = Deployment {
        chain_id: "ckb-testnet".into(),
        genesis_hash: hex::encode([0x11; 32]),
        child_tx_hash: hex::encode([0x22; 32]),
        child_index: 0,
        child_data_hash: hex::encode(counter::hash(b"child")),
        verification_key_hash: hex::encode(counter::hash(b"VK")),
    };
    for (child, key, field) in
        [(&b"other"[..], &b"VK"[..], "child_data_hash"), (&b"child"[..], &b"other"[..], "verification_key_hash")]
    {
        let error = parent(&deployment, child, key).err().expect("substitution must reject").to_string();
        assert!(error.contains(field) && error.contains("expected") && error.contains("observed"), "{error}");
    }
    let mut malformed = serde_json::to_value(deployment).unwrap();
    malformed["child_index"] = serde_json::json!(4294967296u64);
    assert!(serde_json::from_value::<Deployment>(malformed.clone()).is_err());
    malformed["child_index"] = serde_json::json!(0);
    malformed["unrecognized"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Deployment>(malformed).is_err());
}
