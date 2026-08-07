create table if not exists verification_jobs (
  id uuid primary key default gen_random_uuid(),
  namespace text not null,
  name text not null,
  version text not null,
  status text not null default 'queued',
  attempt_count integer not null default 0,
  max_attempts integer not null default 3,
  available_at timestamptz not null default now(),
  lease_owner text,
  lease_expires_at timestamptz,
  evidence_hash text,
  evidence jsonb,
  last_error_code text,
  last_error_message text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  started_at timestamptz,
  completed_at timestamptz,
  unique (namespace, name, version),
  foreign key (namespace, name, version)
    references package_versions(namespace, name, version),
  check (status in ('queued', 'running', 'publishing', 'retry_wait', 'succeeded', 'dead_letter')),
  check (attempt_count >= 0),
  check (max_attempts between 1 and 20),
  check (
    (status in ('running', 'publishing') and lease_owner is not null and lease_expires_at is not null)
    or
    (status not in ('running', 'publishing') and lease_owner is null and lease_expires_at is null)
  ),
  check (
    (evidence_hash is null and evidence is null)
    or
    (evidence_hash ~ '^sha256:[0-9A-Fa-f]{64}$' and evidence is not null)
  ),
  check ((status = 'succeeded') = (completed_at is not null))
);

create index if not exists verification_jobs_claim_idx
  on verification_jobs(status, available_at, lease_expires_at, created_at);

create index if not exists verification_jobs_dead_letter_idx
  on verification_jobs(updated_at desc)
  where status = 'dead_letter';

insert into verification_jobs(namespace, name, version)
select pv.namespace, pv.name, pv.version
from package_versions pv
where pv.status in ('source_published', 'indexed_pending')
  and not exists (
    select 1
    from package_version_evidence pve
    where pve.namespace = pv.namespace
      and pve.name = pv.name
      and pve.version = pv.version
      and pve.kind = 'verified_build'
  )
on conflict (namespace, name, version) do nothing;
