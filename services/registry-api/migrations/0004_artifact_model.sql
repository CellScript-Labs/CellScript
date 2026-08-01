do $$
begin
  if exists (select 1 from package_versions limit 1) then
    raise exception 'artifact model cut requires an empty unreleased package_versions table';
  end if;
end $$;

alter table package_versions
  add column artifact jsonb,
  add column verification_status text,
  add column deployment_status text,
  add column availability_status text,
  alter column edition drop not null,
  alter column compatibility_profile_hash drop not null;

alter table package_versions
  alter column artifact set not null,
  alter column verification_status set not null,
  alter column deployment_status set not null,
  alter column availability_status set not null,
  add constraint package_versions_artifact_object_check check (jsonb_typeof(artifact) = 'object'),
  add constraint package_versions_verification_status_check
    check (verification_status in ('pending', 'verified', 'evidence_required', 'rejected')),
  add constraint package_versions_deployment_status_check
    check (deployment_status in ('not_applicable', 'undeployed', 'deployed', 'chain_verified')),
  add constraint package_versions_availability_status_check
    check (availability_status in ('active', 'deprecated', 'yanked', 'quarantined'));

alter table package_versions
  drop constraint if exists package_versions_edition_check,
  drop constraint if exists package_versions_compatibility_profile_hash_check;

alter table package_versions
  add constraint package_versions_edition_check check (edition is null or edition = '2026'),
  add constraint package_versions_compatibility_profile_hash_check
    check (compatibility_profile_hash is null or compatibility_profile_hash ~ '^(0x)?[0-9A-Fa-f]{64}$');

create index package_versions_artifact_public_idx
  on package_versions(availability_status, verification_status, deployment_status, (artifact->>'kind'), created_at desc);
