alter table package_versions
  add column if not exists registry_environment text not null default 'production',
  add column if not exists chain_network text not null default 'mainnet',
  add column if not exists expires_at timestamptz,
  add column if not exists expired_at timestamptz,
  add column if not exists purge_after timestamptz,
  add column if not exists static_purged_at timestamptz,
  add column if not exists source_purged_at timestamptz;

alter table package_versions
  add constraint package_versions_registry_environment_check
    check (registry_environment in ('production', 'testnet-sandbox')),
  add constraint package_versions_chain_network_check
    check (chain_network in ('mainnet', 'testnet')),
  add constraint package_versions_environment_network_check
    check (
      (registry_environment = 'production' and chain_network = 'mainnet'
        and expires_at is null and purge_after is null)
      or
      (registry_environment = 'testnet-sandbox' and chain_network = 'testnet'
        and expires_at is not null and purge_after is not null and purge_after > expires_at)
    );

create index if not exists package_versions_expiry_idx
  on package_versions(expires_at)
  where expires_at is not null;

create index if not exists package_versions_object_purge_idx
  on package_versions(purge_after, snapshot_hash)
  where purge_after is not null and source_purged_at is null;
