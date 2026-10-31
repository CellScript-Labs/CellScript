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

fn current(state: &SharedSession, epoch: &Arc<()>) -> bool {
    Arc::ptr_eq(&state.lock().unwrap().epoch, epoch)
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
        let revision = snapshot.document_revision(&analyzed_uri);
        snapshot.prepare_navigation_for(&analyzed_uri);
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
                let revision = snapshot.dependency_revision();
                snapshot.analyze_documents();
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
            let publications = {
                let mut state = state.lock().unwrap();
                if !consistent || !Arc::ptr_eq(&state.epoch, &epoch) {
                    continue;
                }
                state.server = snapshot;
                state.worker_running = false;
                versions
                    .into_iter()
                    .map(|(uri, version)| {
                        let diagnostics = state.server.diagnostics.get(&uri).cloned().unwrap_or_default();
                        (uri, version, diagnostics)
                    })
                    .collect::<Vec<_>>()
            };
            for (uri, version, diagnostics) in publications {
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
}
