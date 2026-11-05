include!("ckb_devnet.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};

    fn mock_rpc(replies: Vec<(&'static str, Value)>) -> (CkbDevnet, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let mut methods = Vec::new();
            for (method, result) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = None;
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = Some(value.trim().parse::<usize>().unwrap());
                    }
                }
                let mut body = vec![0; length.unwrap()];
                reader.read_exact(&mut body).unwrap();
                let request: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(request["method"], method, "a transaction must not bypass snapshot synchronization");
                methods.push(method.to_owned());
                let response = serde_json::to_vec(&json!({"id":42,"jsonrpc":"2.0","result":result})).unwrap();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", response.len()).unwrap();
                stream.write_all(&response).unwrap();
                stream.flush().unwrap();
            }
            methods
        });
        let devnet = CkbDevnet {
            ckb_repo: PathBuf::new(),
            ckb_bin: PathBuf::new(),
            ckb_dir: PathBuf::new(),
            log_path: PathBuf::new(),
            rpc_url: format!("http://127.0.0.1:{port}"),
            client: ClientBuilder::new().no_proxy().timeout(Duration::from_secs(2)).build().unwrap(),
            process: None,
            reserved: BTreeSet::new(),
        };
        (devnet, server)
    }

    #[test]
    fn transaction_submission_waits_for_pool_snapshot_and_committed_snapshot() {
        let old = format!("0x{}", "11".repeat(32));
        let new = format!("0x{}", "22".repeat(32));
        let hash = format!("0x{}", "33".repeat(32));
        let (devnet, server) = mock_rpc(vec![
            ("get_tip_header", json!({"hash":new,"number":"0x2"})),
            ("tx_pool_info", json!({"tip_hash":old,"tip_number":"0x1"})),
            ("tx_pool_info", json!({"tip_hash":new,"tip_number":"0x2"})),
            ("send_test_transaction", json!(hash)),
            ("get_transaction", json!({"tx_status":{"status":"committed"}})),
            ("get_tip_header", json!({"hash":new,"number":"0x2"})),
            ("tx_pool_info", json!({"tip_hash":new,"tip_number":"0x1"})),
            ("tx_pool_info", json!({"tip_hash":new,"tip_number":"0x2"})),
        ]);
        assert_eq!(devnet.submit_and_commit(&json!({}), "funding").unwrap()["tx_hash"], hash);
        assert_eq!(server.join().unwrap().iter().filter(|method| method.as_str() == "send_test_transaction").count(), 1);
    }

    #[test]
    fn lagging_pool_times_out_without_submitting_a_transaction() {
        let hash = format!("0x{}", "11".repeat(32));
        let mut replies = vec![("get_tip_header", json!({"hash":hash,"number":"0x2"}))];
        replies.extend((0..80).map(|_| ("tx_pool_info", json!({"tip_hash":hash,"tip_number":"0x1"}))));
        let (devnet, server) = mock_rpc(replies);
        let error = devnet.submit_and_commit(&json!({}), "funding").unwrap_err();
        assert!(error.to_string().contains("transaction pool did not reach chain tip"));
        assert!(!server.join().unwrap().iter().any(|method| method == "send_test_transaction"));
    }

    #[test]
    fn malformed_snapshot_identity_never_submits_a_transaction() {
        let hash = format!("0x{}", "11".repeat(32));
        for response in [json!({}), json!({"tip_hash":"invalid","tip_number":"0x2"}), json!({"tip_hash":hash})] {
            let (devnet, server) = mock_rpc(vec![("get_tip_header", json!({"hash":hash,"number":"0x2"})), ("tx_pool_info", response)]);
            assert!(devnet.submit_and_commit(&json!({}), "funding").is_err());
            assert!(!server.join().unwrap().iter().any(|method| method == "send_test_transaction"));
        }
    }

    #[test]
    fn a_rejected_transaction_is_never_resubmitted() {
        let hash = format!("0x{}", "11".repeat(32));
        let (devnet, server) = mock_rpc(vec![
            ("get_tip_header", json!({"hash":hash,"number":"0x2"})),
            ("tx_pool_info", json!({"tip_hash":hash,"tip_number":"0x2"})),
            ("send_test_transaction", json!(hash)),
            ("get_transaction", json!({"tx_status":{"status":"rejected","reason":"Resolve failed Unknown"}})),
        ]);
        assert!(devnet.submit_and_commit(&json!({}), "funding").unwrap_err().to_string().contains("rejected"));
        assert_eq!(server.join().unwrap().iter().filter(|method| method.as_str() == "send_test_transaction").count(), 1);
    }

    #[test]
    fn entry_witness_helper_places_payload_in_input_type() {
        let payload = b"CSARGv1\0payload";
        let encoded = decode_hex(&entry_witness_input_type_hex(payload)).unwrap();
        let witness = WitnessArgs::from_slice(&encoded).unwrap();

        assert!(witness.lock().to_opt().is_none());
        assert_eq!(witness.input_type().to_opt().unwrap().raw_data(), Bytes::from_static(payload));
        assert!(witness.output_type().to_opt().is_none());
    }
}
