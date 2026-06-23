create extension if not exists pgcrypto;

create table if not exists principals (
  principal_type text not null,
  principal_id text not null,
  display_address text,
  status text not null default 'active',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (principal_type, principal_id),
  check (principal_type = 'joyid_ckb'),
  check (status in ('active', 'suspended'))
);

create table if not exists namespaces (
  namespace text primary key,
  owner_principal_type text not null,
  owner_principal_id text not null,
  status text not null default 'active',
  claimed_at timestamptz not null default now(),
  cooldown_until timestamptz,
  review_reason text,
  audit_request_id text not null,
  check (status in ('active', 'review_pending', 'reserved', 'rejected', 'quarantined'))
);

create table if not exists reserved_namespaces (
  namespace text primary key,
  match_type text not null default 'exact',
  reason text not null,
  created_at timestamptz not null default now(),
  check (match_type in ('exact', 'prefix', 'typosquat'))
);

insert into reserved_namespaces(namespace, match_type, reason) values
  ('admin', 'exact', 'core registry administration namespace'),
  ('api', 'exact', 'production API hostname namespace'),
  ('cellscript', 'exact', 'core CellScript ecosystem namespace'),
  ('ckb', 'exact', 'core CKB ecosystem namespace'),
  ('joyid', 'exact', 'wallet identity provider namespace'),
  ('nervos', 'exact', 'core Nervos ecosystem namespace'),
  ('official', 'exact', 'reserved for official package labels'),
  ('registry', 'exact', 'core registry service namespace'),
  ('security', 'exact', 'reserved for security advisory workflows'),
  ('support', 'exact', 'reserved for support workflows'),
  ('www', 'exact', 'production website hostname namespace')
on conflict (namespace) do nothing;

create table if not exists packages (
  namespace text not null references namespaces(namespace),
  name text not null,
  source_repo text,
  status text not null default 'active',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (namespace, name),
  check (status in ('active', 'review_pending', 'quarantined', 'deprecated'))
);

create table if not exists capabilities (
  key_id text primary key,
  principal_type text not null,
  principal_id text not null,
  capability_pubkey text not null unique,
  scopes text[] not null,
  expires_at timestamptz not null,
  revoked_at timestamptz,
  created_at timestamptz not null default now(),
  authorisation_payload jsonb not null,
  joyid_signature jsonb not null,
  last_used_at timestamptz,
  foreign key (principal_type, principal_id)
    references principals(principal_type, principal_id),
  check (cardinality(scopes) > 0)
);

create table if not exists source_snapshots (
  snapshot_hash text primary key,
  r2_key text not null unique,
  source_hash text not null,
  size_bytes bigint not null,
  content_type text not null,
  created_at timestamptz not null default now(),
  hidden_at timestamptz,
  hidden_reason text,
  check (size_bytes > 0)
);

create table if not exists package_versions (
  namespace text not null,
  name text not null,
  version text not null,
  status text not null,
  source_hash text not null,
  manifest_hash text,
  capability_key_id text not null references capabilities(key_id),
  principal_type text not null,
  principal_id text not null,
  registry_entry jsonb not null,
  snapshot_hash text not null references source_snapshots(snapshot_hash),
  direct_url text not null,
  created_at timestamptz not null default now(),
  indexed_at timestamptz,
  verified_at timestamptz,
  yanked_at timestamptz,
  yanked_reason text,
  quarantined_at timestamptz,
  quarantine_reason text,
  primary key (namespace, name, version),
  foreign key (namespace, name) references packages(namespace, name),
  check (status in (
    'source_published',
    'indexed_pending',
    'verified_build',
    'deployed',
    'on_chain_attested',
    'deprecated',
    'yanked',
    'quarantined'
  ))
);

create table if not exists idempotency_keys (
  key text primary key,
  request_hash text not null,
  request_id text not null,
  status text not null default 'processing',
  response_status integer,
  response jsonb,
  expires_at timestamptz not null,
  created_at timestamptz not null default now(),
  completed_at timestamptz,
  check (status in ('processing', 'completed')),
  check (
    (status = 'processing' and response_status is null and response is null and completed_at is null)
    or
    (status = 'completed' and response_status is not null and response is not null and completed_at is not null)
  )
);

create index if not exists idempotency_keys_expires_idx
  on idempotency_keys(expires_at);

create table if not exists used_nonces (
  nonce_key text primary key,
  protocol text not null,
  action text not null,
  nonce text not null,
  request_id text not null,
  principal_type text,
  principal_id text,
  capability_key_id text,
  expires_at timestamptz not null,
  created_at timestamptz not null default now()
);

create index if not exists used_nonces_expires_idx
  on used_nonces(expires_at);

create index if not exists used_nonces_subject_idx
  on used_nonces(protocol, action, principal_type, principal_id, capability_key_id, created_at desc);

create table if not exists policy_hooks (
  id uuid primary key default gen_random_uuid(),
  hook_type text not null,
  subject_type text not null,
  subject_key text not null,
  status text not null default 'inactive',
  config jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  check (hook_type in ('manual_review', 'quarantine', 'future_bond', 'refundable_deposit')),
  check (subject_type in ('principal', 'capability', 'namespace', 'package', 'package_version', 'global')),
  check (status in ('inactive', 'evaluate', 'enforced', 'disabled'))
);

create unique index if not exists policy_hooks_subject_idx
  on policy_hooks(hook_type, subject_type, subject_key);

create table if not exists bond_policy_hooks (
  policy_hook_id uuid primary key references policy_hooks(id) on delete cascade,
  asset text not null,
  amount text not null,
  refundable boolean not null default true,
  settlement_policy text not null default 'future',
  check (asset <> ''),
  check (amount <> ''),
  check (settlement_policy in ('future', 'manual', 'automatic'))
);

create table if not exists audit_events (
  id uuid primary key default gen_random_uuid(),
  request_id text not null,
  event_type text not null,
  principal_type text,
  principal_id text,
  capability_key_id text,
  namespace text,
  name text,
  version text,
  ip_hash text,
  user_agent text,
  data jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now()
);

create index if not exists audit_events_package_idx
  on audit_events(namespace, name, version, created_at desc);

create index if not exists audit_events_principal_idx
  on audit_events(principal_type, principal_id, created_at desc);

create index if not exists audit_events_type_idx
  on audit_events(event_type, created_at desc);

create index if not exists audit_events_request_idx
  on audit_events(request_id);

create table if not exists quota_events (
  id uuid primary key default gen_random_uuid(),
  quota_key text not null,
  bucket text not null,
  created_at timestamptz not null default now()
);

create index if not exists quota_events_window_idx
  on quota_events(quota_key, bucket, created_at desc);
