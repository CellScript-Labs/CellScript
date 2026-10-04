use cellscript_artifact_checker::zk::{Request, Statement};
#[test]
fn shared_rust_typescript_wire_vectors() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/zk-wire-vectors.json")).unwrap();
    for vector in fixture["vectors"].as_array().unwrap() {
        let bytes = |key: &str| hex::decode(vector[key].as_str().unwrap()).unwrap();
        let statement = Statement::decode(&bytes("statement")).unwrap();
        assert_eq!(statement.encode().as_slice(), bytes("statement"));
        assert_eq!(statement.public_inputs().as_slice(), bytes("public_inputs"));
        let request =
            Request { statement, verification_key: bytes("key").try_into().unwrap(), proof: bytes("proof").try_into().unwrap() };
        assert_eq!(request.encode().as_slice(), bytes("request"));
        assert_eq!(Request::decode(&bytes("request"), &request.verification_key).unwrap(), request);
    }
}
