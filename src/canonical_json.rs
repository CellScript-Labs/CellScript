//! Compiler-side key-sorted JSON. Hash domains remain the caller's contract.
//! Arrays retain order; scalars use serde_json's exact spelling and escaping.

pub fn canonical_json_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => serde_json::Value::Array(items.iter().map(canonical_json_value).collect()),
        serde_json::Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            let mut canonical = serde_json::Map::new();
            for key in keys {
                if let Some(item) = object.get(key) {
                    canonical.insert(key.clone(), canonical_json_value(item));
                }
            }
            serde_json::Value::Object(canonical)
        }
        other => other.clone(),
    }
}

/// A parsed `Value` has string keys and valid JSON numbers. Its serializer has
/// no unsupported values or user implementations, so String serialization is
/// infallible. Raw input parsing remains fallible at each protocol boundary.
pub fn canonical_json_string(value: &serde_json::Value) -> String {
    serde_json::to_string(&canonical_json_value(value)).expect("JSON Value serialization into String is infallible")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_bytes_match_independent_protocol_vectors() {
        let vectors: serde_json::Value =
            serde_json::from_str(cellscript_artifact_checker::CANONICAL_JSON_CONFORMANCE_VECTORS).unwrap();
        for vector in vectors["valid"].as_array().unwrap() {
            let value: serde_json::Value = serde_json::from_str(vector["input"].as_str().unwrap()).unwrap();
            assert_eq!(canonical_json_string(&value), vector["canonical"].as_str().unwrap());
        }
        for input in vectors["invalid"].as_array().unwrap() {
            assert!(serde_json::from_str::<serde_json::Value>(input.as_str().unwrap()).is_err());
        }
    }
}
