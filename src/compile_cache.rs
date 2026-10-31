//! Persistent compile results, source-set identity, suppression and safe eviction.
use super::*;

const ARTIFACT_CACHE_VERSION: &str = "project-source-set-v152-0.33-pipe-status-and-endpoint-results";

thread_local! {
    static INCREMENTAL_CACHE_SUPPRESSION_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Run an in-memory analysis compile without reading, writing, touching, or
/// pruning the persistent incremental cache.
pub(crate) fn without_incremental_cache<T>(operation: impl FnOnce() -> T) -> T {
    INCREMENTAL_CACHE_SUPPRESSION_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
    struct RestoreIncrementalCache;
    impl Drop for RestoreIncrementalCache {
        fn drop(&mut self) {
            INCREMENTAL_CACHE_SUPPRESSION_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
        }
    }
    let _restore = RestoreIncrementalCache;
    operation()
}

pub(super) fn incremental_cache_enabled() -> bool {
    INCREMENTAL_CACHE_SUPPRESSION_DEPTH.with(|depth| depth.get() == 0) && !offline_environment_active()
}

#[cfg(feature = "cli")]
pub(crate) fn refresh_incremental_cache_for_input<P: AsRef<Utf8Path>>(
    input: P,
    options: &CompileOptions,
    result: &CompileResult,
) -> Result<()> {
    let resolved = resolve_input_path(input.as_ref())?;
    let source_units = collect_source_units_for_compile_file(&resolved)?;
    let cache_units = collect_cache_units_for_compile_file(&resolved, &source_units)?;
    // A package's mandatory edition overrides the caller's standalone default.
    // Rebuilding Cell.lock must refresh the same cache key that compilation and
    // build-plan use, including the source-evidence mode.
    let mut resolved_options = options.clone();
    resolved_options.edition = result.metadata.edition;
    incremental_cache_store(&resolved, &cache_units, &resolved_options, result);
    Ok(())
}

/// Check incremental compilation cache for a previous compile result.
/// Returns `Some(result)` if the cache is valid and the source has not changed.
pub(super) fn incremental_cache_hit(
    path: &Utf8Path,
    cache_units: &[SourceUnitMetadata],
    options: &CompileOptions,
) -> Option<CompileResult> {
    incremental_cache_result(path, cache_units, options, true)
}

pub(super) fn incremental_cache_result(
    path: &Utf8Path,
    cache_units: &[SourceUnitMetadata],
    options: &CompileOptions,
    refresh_recency: bool,
) -> Option<CompileResult> {
    let max_entries = incremental_cache_max_entries();
    if max_entries == 0 {
        return None;
    }
    let cache_dir = incremental_cache_dir(path)?;
    let cache_key = incremental_cache_key(cache_units, options);
    let entry_dir = cache_dir.join(&cache_key);

    let entry_metadata = std::fs::symlink_metadata(&entry_dir).ok()?;
    if entry_metadata.file_type().is_symlink() || !entry_metadata.is_dir() {
        return None;
    }

    let artifact_path = entry_dir.join("artifact");
    let metadata_path = entry_dir.join("metadata.json");
    let lowering_record_path = entry_dir.join("lowering.json");
    let source_map_path = entry_dir.join("sourcemap.json");
    let source_hash_file = entry_dir.join("source_hash");

    if ![&artifact_path, &metadata_path, &source_hash_file].into_iter().all(|path| incremental_cache_file_is_regular(path)) {
        return None;
    }

    // Verify the source content hash still matches.
    let cached_source_hash = std::fs::read_to_string(&source_hash_file).ok()?;
    let current_source_hash = source_set_hash(cache_units);
    if cached_source_hash != current_source_hash {
        return None;
    }

    // Load cached artifact and metadata.
    let artifact_bytes = std::fs::read(&artifact_path).ok()?;
    let metadata_json = std::fs::read_to_string(&metadata_path).ok()?;
    let metadata: CompileMetadata = serde_json::from_str(&metadata_json).ok()?;
    let (verified_lowering_record, source_artifact_map) = if metadata.artifact_format == "RISC-V ELF" {
        if !incremental_cache_file_is_regular(&lowering_record_path) || !incremental_cache_file_is_regular(&source_map_path) {
            return None;
        }
        let lowering_bytes = std::fs::read(&lowering_record_path).ok()?;
        let source_map_bytes = std::fs::read(&source_map_path).ok()?;
        (
            Some(cellscript_artifact_checker::parse_lowering_record(&lowering_bytes, &CheckerBudgets::default()).ok()?),
            Some(cellscript_artifact_checker::parse_source_map(&source_map_bytes, &CheckerBudgets::default()).ok()?),
        )
    } else {
        (None, None)
    };

    let artifact_hash: [u8; 32] = {
        let hash_hex = metadata.artifact_hash.as_deref().unwrap_or("");
        hex_decode(hash_hex).unwrap_or([0u8; 32])
    };

    let result = CompileResult {
        artifact_bytes,
        artifact_format: match metadata.artifact_format.as_str() {
            "RISC-V ELF" => ArtifactFormat::RiscvElf,
            _ => ArtifactFormat::RiscvAssembly,
        },
        artifact_hash,
        metadata,
        ast: ast::Module {
            name: String::new(),
            items: Vec::new(),
            interface_templates: Vec::new(),
            visibilities: BTreeMap::new(),
            span: crate::error::Span { start: 0, end: 0, line: 0, column: 0 },
        },
        verified_lowering_record,
        source_artifact_map,
        verified_artifact_draft: None,
        cache_hit: true,
    };
    result.validate().ok()?;
    if refresh_recency {
        refresh_incremental_cache_recency(&entry_dir);
        prune_incremental_cache_entries(&cache_dir, &cache_key, max_entries);
    }
    Some(result)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IncrementalCacheInspection {
    pub cache_key: String,
    pub source_set_hash: String,
    pub status: String,
    pub rebuild_reason: String,
}

pub(crate) fn inspect_incremental_cache<P: AsRef<Utf8Path>>(input: P, options: &CompileOptions) -> Result<IncrementalCacheInspection> {
    let resolved = resolve_input_path(input.as_ref())?;
    let source_units = collect_source_units_for_compile_file(&resolved)?;
    let cache_units = collect_cache_units_for_compile_file(&resolved, &source_units)?;
    let cache_key = incremental_cache_key(&cache_units, options);
    let source_set_hash = source_set_hash(&cache_units);
    let cache_entry_exists =
        incremental_cache_dir(&resolved).map(|directory| directory.join(&cache_key)).is_some_and(|entry| entry.exists());
    let up_to_date = incremental_cache_result(&resolved, &cache_units, options, false).is_some();
    let (status, rebuild_reason) = if up_to_date {
        ("up-to-date", "cache-entry-valid")
    } else if cache_entry_exists {
        ("stale", "cache-entry-invalid")
    } else {
        ("missing", "cache-entry-missing")
    };
    Ok(IncrementalCacheInspection {
        cache_key,
        source_set_hash,
        status: status.to_string(),
        rebuild_reason: rebuild_reason.to_string(),
    })
}

/// Store compile result to incremental cache after a successful compile.
pub(super) fn incremental_cache_store(
    path: &Utf8Path,
    cache_units: &[SourceUnitMetadata],
    options: &CompileOptions,
    result: &CompileResult,
) {
    let max_entries = incremental_cache_max_entries();
    if max_entries == 0 {
        return;
    }
    let Some(cache_dir) = incremental_cache_dir(path) else { return };
    let cache_key = incremental_cache_key(cache_units, options);
    let entry_dir = cache_dir.join(&cache_key);

    let mut files = vec![
        ("artifact", result.artifact_bytes.clone()),
        (
            "metadata.json",
            match serde_json::to_vec_pretty(&result.metadata) {
                Ok(bytes) => bytes,
                Err(_) => return,
            },
        ),
        ("source_hash", source_set_hash(cache_units).into_bytes()),
        (".last_used", Vec::new()),
    ];
    if let Some(record) = &result.verified_lowering_record {
        let Ok(bytes) = cellscript_artifact_checker::canonical_bytes(record) else { return };
        files.push(("lowering.json", bytes));
    }
    if let Some(source_map) = &result.source_artifact_map {
        let Ok(bytes) = cellscript_artifact_checker::canonical_bytes(source_map) else { return };
        files.push(("sourcemap.json", bytes));
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(cache_units) {
        files.push(("source_units.json", bytes));
    }

    if !create_incremental_cache_components(&cache_dir) || std::fs::create_dir(&entry_dir).is_err() {
        return;
    }
    if !files.into_iter().all(|(name, bytes)| write_new_incremental_cache_file(&entry_dir.join(name), &bytes)) {
        if std::fs::symlink_metadata(&entry_dir).is_ok_and(|metadata| !metadata.file_type().is_symlink() && metadata.is_dir()) {
            let _ = std::fs::remove_dir_all(&entry_dir);
        }
        return;
    }
    prune_incremental_cache_entries(&cache_dir, &cache_key, max_entries);
}

pub(super) fn incremental_cache_file_is_regular(path: &Utf8Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.file_type().is_symlink() && metadata.is_file())
}

pub(super) fn write_new_incremental_cache_file(path: &Utf8Path, bytes: &[u8]) -> bool {
    use std::io::Write;

    let Ok(mut file) = std::fs::OpenOptions::new().write(true).create_new(true).open(path) else { return false };
    file.write_all(bytes).is_ok()
}

pub(super) fn refresh_incremental_cache_recency(entry_dir: &Utf8Path) {
    let nonce =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or_default();
    let temporary = entry_dir.join(format!(".cellscript-cache-touch-{}-{nonce}.tmp", std::process::id()));
    if !write_new_incremental_cache_file(&temporary, &[]) {
        return;
    }
    if std::fs::rename(&temporary, entry_dir.join(".last_used")).is_err() {
        let _ = std::fs::remove_file(temporary);
    }
}

const DEFAULT_INCREMENTAL_CACHE_MAX_ENTRIES: usize = 32;
const MAX_INCREMENTAL_CACHE_MAX_ENTRIES: usize = 4_096;

pub(super) fn incremental_cache_max_entries() -> usize {
    std::env::var("CELLSCRIPT_INCREMENTAL_CACHE_MAX_ENTRIES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value <= MAX_INCREMENTAL_CACHE_MAX_ENTRIES)
        .unwrap_or(DEFAULT_INCREMENTAL_CACHE_MAX_ENTRIES)
}

pub(super) fn cache_entry_recency(path: &std::path::Path) -> std::time::SystemTime {
    std::fs::symlink_metadata(path.join(".last_used"))
        .and_then(|metadata| {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsafe incremental-cache recency marker"))
            } else {
                metadata.modified()
            }
        })
        .or_else(|_| std::fs::symlink_metadata(path).and_then(|metadata| metadata.modified()))
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
}

pub(super) fn prune_incremental_cache_entries(cache_dir: &Utf8Path, current_key: &str, max_entries: usize) {
    if max_entries == 0 || !cache_dir.is_dir() || !incremental_cache_components_are_safe(cache_dir) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(cache_dir) else { return };
    let mut entries = entries
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            if name.len() != 64 || !name.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            let metadata = std::fs::symlink_metadata(entry.path()).ok()?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return None;
            }
            let recency = cache_entry_recency(&entry.path());
            Some((name, recency, entry.path()))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| right.0.cmp(&left.0)));

    let mut retained = 0_usize;
    for (name, _, path) in entries {
        if name == current_key || retained < max_entries.saturating_sub(1) {
            retained += usize::from(name != current_key);
            continue;
        }
        let _ = std::fs::remove_dir_all(path);
    }
}

pub(super) fn incremental_cache_dir(path: &Utf8Path) -> Option<Utf8PathBuf> {
    // Prefer the package root (where Cell.toml lives) so the cache sits at
    //   <pkg-root>/.cell/build/cache
    // rather than next to the source file (e.g. src/.cell/build/cache).
    // Falls back to the source file's parent when not inside a package.
    let cache_dir = if let Ok(Some(pkg_root)) = find_package_root(path) {
        pkg_root.join(".cell/build/cache")
    } else {
        path.parent()?.join(".cell/build/cache")
    };
    incremental_cache_components_are_safe(&cache_dir).then_some(cache_dir)
}

pub(super) fn incremental_cache_components_are_safe(cache_dir: &Utf8Path) -> bool {
    let Some(build_dir) = cache_dir.parent() else { return false };
    let Some(cell_dir) = build_dir.parent() else { return false };
    if cache_dir.file_name() != Some("cache") || build_dir.file_name() != Some("build") || cell_dir.file_name() != Some(".cell") {
        return false;
    }
    [cell_dir, build_dir, cache_dir].into_iter().all(|path| match std::fs::symlink_metadata(path) {
        Ok(metadata) => !metadata.file_type().is_symlink() && metadata.is_dir(),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    })
}

pub(super) fn create_incremental_cache_components(cache_dir: &Utf8Path) -> bool {
    let Some(build_dir) = cache_dir.parent() else { return false };
    let Some(cell_dir) = build_dir.parent() else { return false };
    if cache_dir.file_name() != Some("cache") || build_dir.file_name() != Some("build") || cell_dir.file_name() != Some(".cell") {
        return false;
    }
    for path in [cell_dir, build_dir, cache_dir] {
        match std::fs::create_dir(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let Ok(metadata) = std::fs::symlink_metadata(path) else { return false };
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }
    incremental_cache_components_are_safe(cache_dir)
}

pub(super) fn incremental_cache_key(cache_units: &[SourceUnitMetadata], options: &CompileOptions) -> String {
    let mut key_input = String::new();
    key_input.push_str("cellscript-incremental-cache-v2");
    key_input.push_str(&format!("-compiler-{}", VERSION));
    key_input.push_str(&format!("-metadata-{}", METADATA_SCHEMA_VERSION));
    key_input.push_str(&format!(
        "-metadata-components-{}-{}-{}",
        SOURCE_METADATA_SCHEMA_VERSION, ARTIFACT_METADATA_SCHEMA_VERSION, CONSTRAINTS_METADATA_SCHEMA_VERSION
    ));
    key_input.push_str(&format!("-artifact-cache-{}", ARTIFACT_CACHE_VERSION));
    key_input.push_str(&source_set_hash(cache_units));
    key_input.push_str(COMPILATION_POLICY);
    key_input.push_str(&format!("-diagnostic-{}", options.diagnostics.cache_bits()));
    key_input.push_str(&format!("-{}", options.target.as_deref().unwrap_or("default")));
    key_input.push_str(&format!("-{}", options.target_profile.as_deref().unwrap_or("default")));
    key_input.push_str(&format!("-edition-{}", options.edition));
    let target_profile = options.target_profile.as_deref().unwrap_or(DEFAULT_TARGET_PROFILE);
    let compatibility_profile = resolve_compatibility_profile(options.edition, target_profile, options.primitive_compat.as_deref());
    key_input.push_str(&format!("-compatibility-profile-{}", compatibility_profile.id));
    key_input.push_str(&format!("-debug{}", options.debug));
    key_input.push_str(&format!("-source-contracts{}", options.source_contracts));
    key_input.push_str(&format!("-primitive-{}", options.primitive_compat.as_deref().unwrap_or("default")));
    hex_encode(&ckb_blake2b256(key_input.as_bytes()))
}

pub(super) fn source_set_hash(cache_units: &[SourceUnitMetadata]) -> String {
    let mut key_input = String::new();
    for unit in cache_units {
        key_input.push_str(&unit.role);
        key_input.push('\0');
        key_input.push_str(&unit.path);
        key_input.push('\0');
        key_input.push_str(&unit.hash);
        key_input.push('\0');
        key_input.push_str(&unit.size_bytes.to_string());
        key_input.push('\n');
    }
    hex_encode(&ckb_blake2b256(key_input.as_bytes()))
}
