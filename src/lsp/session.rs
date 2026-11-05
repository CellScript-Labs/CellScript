//! Native session revisions and one coalescing diagnostic worker. Mutable
//! buffers are protected briefly; compilation and queries own their snapshots.
use super::*;
use std::sync::{Arc, Mutex};
use tower_lsp::{jsonrpc, Client};

pub(super) struct Session {
    pub(super) server: LspServer,
    versions: HashMap<String, i32>,
    epoch: Arc<()>,
    worker_running: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self { server: LspServer::new(), versions: HashMap::new(), epoch: Arc::new(()), worker_running: false }
    }
}

impl Session {
    pub(super) fn open(&mut self, uri: String, version: i32, source: String) {
        self.versions.insert(uri.clone(), version);
        self.server.edition_overrides.remove(&uri);
        self.server.replace_buffer(uri, source);
        self.epoch = Arc::new(());
    }

    pub(super) fn change(&mut self, uri: &str, version: i32, changes: Vec<TextDocumentContentChangeEvent>) -> bool {
        if self.versions.get(uri).is_none_or(|previous| version <= *previous) {
            return false;
        }
        self.versions.insert(uri.into(), version);
        self.server.apply_buffer_changes(uri, changes);
        self.epoch = Arc::new(());
        true
    }

    pub(super) fn close(&mut self, uri: &str) {
        self.versions.remove(uri);
        self.server.close_document(uri);
        self.epoch = Arc::new(());
    }

    pub(super) fn dependencies_changed(&mut self) {
        self.server.ast_cache.clear();
        self.epoch = Arc::new(());
    }
}

pub(super) type SharedSession = Arc<Mutex<Session>>;

struct DiagnosticPublication {
    uri: String,
    version: i32,
    diagnostics: Arc<[Diagnostic]>,
}

fn current(state: &SharedSession, epoch: &Arc<()>) -> bool {
    Arc::ptr_eq(&state.lock().unwrap().epoch, epoch)
}

fn install_analysis(
    state: &SharedSession,
    epoch: &Arc<()>,
    versions: HashMap<String, i32>,
    snapshot: LspServer,
    consistent: bool,
) -> Option<Vec<DiagnosticPublication>> {
    let mut state = state.lock().unwrap();
    if !consistent || !Arc::ptr_eq(&state.epoch, epoch) {
        return None;
    }
    state.server = snapshot;
    state.worker_running = false;
    Some(
        versions
            .into_iter()
            .map(|(uri, version)| {
                let diagnostics = state.server.diagnostics.get(&uri).cloned().unwrap_or_default();
                DiagnosticPublication { uri, version, diagnostics }
            })
            .collect(),
    )
}

pub(super) async fn query<T: Send + 'static>(
    state: &SharedSession,
    uri: String,
    operation: impl FnOnce(&LspServer) -> T + Send + 'static,
) -> jsonrpc::Result<T> {
    let (epoch, version, mut snapshot) = {
        let state = state.lock().unwrap();
        (state.epoch.clone(), state.versions.get(&uri).copied(), state.server.clone())
    };
    let analyzed_uri = uri.clone();
    let (result, revision, consistent) = tokio::task::spawn_blocking(move || {
        let revision = snapshot.prepare_navigation_snapshot(&analyzed_uri);
        let result = operation(&snapshot);
        (result, revision, snapshot.document_revision(&analyzed_uri) == revision)
    })
    .await
    .map_err(|_| jsonrpc::Error::internal_error())?;
    if !consistent {
        return Err(jsonrpc::Error::request_cancelled());
    }
    let changed = {
        let state = state.lock().unwrap();
        if state.versions.get(&uri).copied() != version {
            return Err(jsonrpc::Error::request_cancelled());
        }
        (!Arc::ptr_eq(&state.epoch, &epoch)).then(|| (state.epoch.clone(), state.server.clone()))
    };
    // An unrelated document edit need not cancel this request. Compare the
    // actual source/manifest dependency set against the latest buffer snapshot.
    if let Some((epoch, latest)) = changed {
        let current =
            tokio::task::spawn_blocking(move || latest.document_revision(&uri)).await.map_err(|_| jsonrpc::Error::internal_error())?;
        if current != revision || !self::current(state, &epoch) {
            return Err(jsonrpc::Error::request_cancelled());
        }
    }
    Ok(result)
}

pub(super) fn schedule(state: &SharedSession, client: &Client) {
    {
        let mut state = state.lock().unwrap();
        if state.worker_running {
            return;
        }
        state.worker_running = true;
    }
    let state = state.clone();
    let client = client.clone();
    tokio::spawn(async move {
        loop {
            let (epoch, versions, mut snapshot) = {
                let state = state.lock().unwrap();
                (state.epoch.clone(), state.versions.clone(), state.server.clone())
            };
            let analyzed = tokio::task::spawn_blocking(move || {
                let revision = snapshot.analyze_documents();
                let consistent = snapshot.dependency_revision() == revision;
                (snapshot, consistent)
            })
            .await;
            let Ok((snapshot, consistent)) = analyzed else {
                let retry = {
                    let mut state = state.lock().unwrap();
                    let retry = !Arc::ptr_eq(&state.epoch, &epoch);
                    if !retry {
                        state.worker_running = false;
                    }
                    retry
                };
                client.log_message(tower_lsp::lsp_types::MessageType::ERROR, "CellScript diagnostic worker failed").await;
                if retry {
                    continue;
                }
                return;
            };
            let Some(publications) = install_analysis(&state, &epoch, versions, snapshot, consistent) else { continue };
            for DiagnosticPublication { uri, version, diagnostics } in publications {
                if !current(&state, &epoch) {
                    return;
                }
                if let Ok(uri) = tower_lsp::lsp_types::Url::parse(&uri) {
                    // Version is mandatory: an edit can arrive while the RPC
                    // notification is being queued after the revision check.
                    client
                        .publish_diagnostics(
                            uri,
                            diagnostics.iter().cloned().map(super::server::convert_diagnostic).collect(),
                            Some(version),
                        )
                        .await;
                }
            }
            return;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_read_fixture() -> (tempfile::TempDir, SharedSession, Vec<String>) {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(root.path().join("Cell.toml"), "[package]\nname = \"workspace_reads\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\n").unwrap();
        let state: SharedSession = Arc::default();
        let mut uris = Vec::new();
        for index in 0..6 {
            let path = src.join(format!("unit{index}.cell"));
            let source = format!("module unit{index}\npublic struct Value{index} {{ amount: u64 }}");
            std::fs::write(&path, &source).unwrap();
            let uri = utf8_path_to_file_uri(camino::Utf8Path::from_path(&path).unwrap());
            if index < 2 {
                state.lock().unwrap().open(uri.clone(), 1, source);
            }
            uris.push(uri);
        }
        (root, state, uris)
    }

    #[test]
    fn client_versions_are_monotonic_and_reopening_gets_a_fresh_epoch() {
        let mut session = Session::default();
        let uri = "untitled:versioned";
        session.open(uri.into(), 10, "module first".into());
        let epoch = session.epoch.clone();
        for version in [10, 9] {
            assert!(!session.change(
                uri,
                version,
                vec![TextDocumentContentChangeEvent { range: None, range_length: None, text: "module stale".into() }]
            ));
        }
        assert!(Arc::ptr_eq(&epoch, &session.epoch));
        assert_eq!(session.server.documents[uri].as_str(), "module first");
        session.close(uri);
        assert!(!Arc::ptr_eq(&epoch, &session.epoch));
        session.open(uri.into(), 1, "module reopened".into());
        assert_eq!(session.versions[uri], 1);
        assert!(!Arc::ptr_eq(&epoch, &session.epoch));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn blocked_analysis_keeps_queries_and_edits_responsive_and_rejects_stale_results() {
        let state: SharedSession = Arc::default();
        let uri = "untitled:query";
        state.lock().unwrap().open(uri.into(), 1, "module first".into());
        let (entered, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let old = state.clone();
        let blocked = tokio::spawn(async move {
            query(&old, uri.into(), move |server| {
                entered.send(()).unwrap();
                wait.recv().unwrap();
                server.documents[uri].as_ref().clone()
            })
            .await
        });
        ready.await.unwrap();
        let response = tokio::time::timeout(std::time::Duration::from_secs(5), query(&state, uri.into(), |_| 42)).await;
        assert_eq!(response.unwrap().unwrap(), 42);
        assert!(state.lock().unwrap().change(
            uri,
            2,
            vec![TextDocumentContentChangeEvent { range: None, range_length: None, text: "module latest".into() }]
        ));
        release.send(()).unwrap();
        assert_eq!(blocked.await.unwrap().unwrap_err().code, jsonrpc::ErrorCode::RequestCancelled);
        assert_eq!(query(&state, uri.into(), |server| server.documents[uri].as_ref().clone()).await.unwrap(), "module latest");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unrelated_buffer_edits_do_not_cancel_a_consistent_query() {
        let state: SharedSession = Arc::default();
        let uri = "untitled:independent";
        state.lock().unwrap().open(uri.into(), 1, "module first".into());
        let (entered, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let old = state.clone();
        let blocked = tokio::spawn(async move {
            query(&old, uri.into(), move |server| {
                entered.send(()).unwrap();
                wait.recv().unwrap();
                server.documents[uri].as_ref().clone()
            })
            .await
        });
        ready.await.unwrap();
        state.lock().unwrap().open("untitled:other".into(), 1, "module other".into());
        release.send(()).unwrap();
        assert_eq!(blocked.await.unwrap().unwrap(), "module first");
        let parsed =
            query(&state, uri.into(), |server| (server.ast_cache.contains_key(uri), server.ast_cache.contains_key("untitled:other")))
                .await
                .unwrap();
        assert_eq!(parsed, (true, false), "an unrelated buffer must not be parsed by this query");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn actual_navigation_query_reads_and_hashes_each_dependency_once_per_observation() {
        use std::sync::atomic::Ordering;
        let (_root, state, uris) = source_read_fixture();
        let stats = state.lock().unwrap().server.read_stats.clone();
        let target = uris[0].clone();
        assert!(query(&state, target.clone(), move |server| server.view_index(&target).unwrap().resolve_type("Value0").is_some())
            .await
            .unwrap());
        assert_eq!(stats.observations.load(Ordering::Relaxed), 2, "one input capture and one fresh consistency check");
        assert_eq!(stats.source_reads.load(Ordering::Relaxed), 8, "four disk dependencies are read once per observation");
        assert_eq!(stats.source_hashes.load(Ordering::Relaxed), 12, "six source buffers are hashed once per observation");
        assert_eq!(stats.source_parses.load(Ordering::Relaxed), 6, "the fresh consistency check can reuse the established graph");
    }

    #[test]
    fn actual_diagnostic_batch_shares_dependency_reads_hashes_and_parsed_modules() {
        use std::sync::atomic::Ordering;
        let (_root, state, uris) = source_read_fixture();
        let mut snapshot = state.lock().unwrap().server.clone();
        let stats = snapshot.read_stats.clone();
        let revision = snapshot.analyze_documents();
        assert!(snapshot.observed_inputs.is_none(), "a completed analysis must not install captured inputs as live state");
        assert_eq!(snapshot.dependency_revision(), revision);
        for uri in uris.iter().take(2) {
            assert!(snapshot.get_diagnostics(uri).iter().all(|diagnostic| diagnostic.severity != DiagnosticSeverity::Error));
        }
        assert_eq!(stats.observations.load(Ordering::Relaxed), 2);
        assert_eq!(stats.source_reads.load(Ordering::Relaxed), 8);
        assert_eq!(stats.source_hashes.load(Ordering::Relaxed), 12);
        assert_eq!(stats.source_parses.load(Ordering::Relaxed), 6, "entry parsing, diagnostics and navigation share source ASTs");
    }

    #[test]
    fn captured_diagnostics_do_not_reopen_changed_manifests_or_removed_dependency_sources() {
        let (root, state, uris) = source_read_fixture();
        let mut snapshot = state.lock().unwrap().server.clone();
        let revision = snapshot.prepare_diagnostic_snapshot();
        std::fs::remove_file(root.path().join("src/unit5.cell")).unwrap();
        std::fs::write(root.path().join("Cell.toml"), "[package\n").unwrap();
        snapshot.analyze_captured_documents();
        for uri in uris.iter().take(2) {
            assert!(snapshot.get_diagnostics(uri).iter().all(|diagnostic| diagnostic.severity != DiagnosticSeverity::Error));
        }
        assert_ne!(snapshot.dependency_revision(), revision, "a fresh check must reject publication of this captured result");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn blocked_workspace_io_keeps_the_session_mutex_available_for_queries_and_edits() {
        struct ReleaseOnDrop(Option<std::sync::mpsc::Sender<()>>);
        impl Drop for ReleaseOnDrop {
            fn drop(&mut self) {
                if let Some(sender) = self.0.take() {
                    let _ = sender.send(());
                }
            }
        }
        let (root, state, uris) = source_read_fixture();
        state.lock().unwrap().open("untitled:fast".into(), 1, "module fast".into());
        let stats = state.lock().unwrap().server.read_stats.clone();
        let (entered, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let mut release = ReleaseOnDrop(Some(release));
        *stats.blocker.lock().unwrap() = Some(workspace::ReadBlocker {
            path: Utf8PathBuf::from_path_buf(root.path().join("src/unit5.cell").canonicalize().unwrap()).unwrap(),
            entered,
            release: wait,
        });
        let old = state.clone();
        let target = uris[0].clone();
        let blocked = tokio::spawn(async move { query(&old, target, |_| 17).await });
        tokio::time::timeout(std::time::Duration::from_secs(5), ready).await.unwrap().unwrap();
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(5), query(&state, "untitled:fast".into(), |_| 42))
                .await
                .unwrap()
                .unwrap(),
            42
        );
        let editing = state.clone();
        let target = uris[0].clone();
        let edit = tokio::task::spawn_blocking(move || {
            editing.lock().unwrap().change(
                &target,
                2,
                vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "module unit0\npublic struct Value0 { amount: u32 }".into(),
                }],
            )
        });
        assert!(tokio::time::timeout(std::time::Duration::from_secs(5), edit).await.unwrap().unwrap());
        release.0.take().unwrap().send(()).unwrap();
        assert_eq!(blocked.await.unwrap().unwrap_err().code, jsonrpc::ErrorCode::RequestCancelled);
    }

    #[test]
    fn stale_or_inconsistent_diagnostics_cannot_replace_session_state_or_select_publications() {
        let state: SharedSession = Arc::default();
        let uri = "untitled:publication";
        state.lock().unwrap().open(uri.into(), 1, "module publication\naction verify() -> u64 { verification return true }".into());
        let (epoch, versions, mut snapshot) = {
            let mut session = state.lock().unwrap();
            session.worker_running = true;
            (session.epoch.clone(), session.versions.clone(), session.server.clone())
        };
        snapshot.analyze_documents();
        assert!(snapshot.get_diagnostics(uri).iter().any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error));
        assert!(install_analysis(&state, &epoch, versions.clone(), snapshot.clone(), false).is_none());
        assert!(state.lock().unwrap().worker_running);
        assert!(state.lock().unwrap().change(
            uri,
            2,
            vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "module publication\naction verify() -> u64 { verification return 7 }".into(),
            }]
        ));
        assert!(install_analysis(&state, &epoch, versions.clone(), snapshot.clone(), true).is_none());
        assert!(state.lock().unwrap().server.get_diagnostics(uri).is_empty());
        state.lock().unwrap().close(uri);
        assert!(install_analysis(&state, &epoch, versions, snapshot, true).is_none());
        assert!(!state.lock().unwrap().server.documents.contains_key(uri));
        let (epoch, versions, mut latest) = {
            let mut session = state.lock().unwrap();
            session.open(uri.into(), 1, "module publication\naction verify() -> u64 { verification return 7 }".into());
            (session.epoch.clone(), session.versions.clone(), session.server.clone())
        };
        latest.analyze_documents();
        let publications = install_analysis(&state, &epoch, versions, latest, true).unwrap();
        assert_eq!(publications.len(), 1);
        assert_eq!((&publications[0].uri, publications[0].version), (&uri.to_owned(), 1));
        assert!(
            publications[0].diagnostics.iter().all(|diagnostic| diagnostic.severity != DiagnosticSeverity::Error),
            "{:?}",
            publications[0].diagnostics
        );
        assert!(!state.lock().unwrap().worker_running);
    }

    #[test]
    fn canonical_source_aliases_reject_conflicts_and_recover_when_bytes_agree() {
        let (_root, state, uris) = source_read_fixture();
        let alias = uris[0].replace("unit0.cell", "%75nit0.cell");
        let original = state.lock().unwrap().server.documents[&uris[0]].as_ref().clone();
        state.lock().unwrap().open(alias.clone(), 1, original.clone());
        let mut snapshot = state.lock().unwrap().server.clone();
        snapshot.analyze_documents();
        assert!(snapshot.get_diagnostics(&uris[0]).iter().all(|diagnostic| diagnostic.severity != DiagnosticSeverity::Error));
        state.lock().unwrap().change(
            &alias,
            2,
            vec![TextDocumentContentChangeEvent { range: None, range_length: None, text: original.replace("u64", "u32") }],
        );
        let mut snapshot = state.lock().unwrap().server.clone();
        snapshot.analyze_documents();
        for uri in [&uris[0], &alias] {
            assert!(snapshot.get_diagnostics(uri).iter().any(|diagnostic| diagnostic.code.as_deref() == Some("E1031")));
        }
        state.lock().unwrap().change(
            &alias,
            3,
            vec![TextDocumentContentChangeEvent { range: None, range_length: None, text: original }],
        );
        state.lock().unwrap().server.edition_overrides.insert(alias.clone(), crate::CellScriptEdition::Edition2027);
        let mut snapshot = state.lock().unwrap().server.clone();
        snapshot.analyze_documents();
        for uri in [&uris[0], &alias] {
            assert!(snapshot.get_diagnostics(uri).iter().all(|diagnostic| diagnostic.code.as_deref() != Some("E1031")));
        }
    }
}
