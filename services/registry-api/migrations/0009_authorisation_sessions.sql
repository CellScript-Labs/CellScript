create table if not exists authorisation_sessions (
  session_id text primary key,
  poll_token_hash text not null,
  browser_token_hash text not null,
  registry_origin text not null,
  website_origin text not null,
  capability_pubkey text not null,
  requested_scopes text[] not null,
  capability_expires_at timestamptz not null,
  cli_version text not null,
  namespace text not null,
  name text not null,
  artifact_kind text not null,
  status text not null default 'pending',
  principal_type text,
  principal_id text,
  payload jsonb,
  challenge_token_hash text,
  capability_key_id text references capabilities(key_id),
  namespace_status text,
  audit_request_id text not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  expires_at timestamptz not null,
  completed_at timestamptz,
  check (session_id ~ '^auth_[0-9a-f]{32}$'),
  check (poll_token_hash ~ '^sha256:[0-9a-f]{64}$'),
  check (browser_token_hash ~ '^sha256:[0-9a-f]{64}$'),
  check (cardinality(requested_scopes) > 0),
  check (artifact_kind in (
    'source_library', 'profile_library', 'runtime_verifier',
    'deployable_contract', 'reproducible_binary', 'template'
  )),
  check (status in ('pending', 'authorised', 'review_pending')),
  check (namespace_status is null or namespace_status in ('active', 'review_pending')),
  check (
    (status = 'pending' and capability_key_id is null and namespace_status is null and completed_at is null)
    or
    (status in ('authorised', 'review_pending') and capability_key_id is not null
      and namespace_status is not null and completed_at is not null)
  )
);

create index if not exists authorisation_sessions_expiry_idx
  on authorisation_sessions(expires_at);
