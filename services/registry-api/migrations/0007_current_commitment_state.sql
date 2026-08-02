alter table package_version_evidence
  drop constraint if exists package_version_evidence_kind_check;

alter table package_versions
  drop constraint if exists package_versions_status_check;

update package_version_evidence
set kind = 'on_chain_committed'
where kind = 'on_chain_attested';

update package_versions
set status = 'on_chain_committed'
where status = 'on_chain_attested';

alter table package_versions
  add column current_commitment_kind text not null default 'on_chain_committed',
  add column current_commitment_evidence_hash text;

-- Historical attestation evidence is not proof that its Cell is still live.
-- Preserve the evidence after renaming it, but fail closed until the mainnet
-- reconciliation job observes a sufficiently confirmed live commitment again.
update package_versions
set status = case
  when availability_status <> 'active' then availability_status
  when deployment_status in ('deployed', 'chain_verified') then 'deployed'
  when verification_status in ('hash_bound', 'verified', 'evidence_required') then 'verified_build'
  when status = 'indexed_pending' then 'indexed_pending'
  else 'source_published'
end;

alter table package_version_evidence
  add constraint package_version_evidence_kind_check
    check (kind in ('verified_build', 'reproduced_build', 'deployed', 'on_chain_committed'));

alter table package_versions
  add constraint package_versions_status_check
    check (status in (
      'source_published',
      'indexed_pending',
      'verified_build',
      'deployed',
      'on_chain_committed',
      'deprecated',
      'yanked',
      'quarantined'
    )),
  add constraint package_versions_current_commitment_kind_check
    check (current_commitment_kind = 'on_chain_committed'),
  add constraint package_versions_current_commitment_evidence_fk
    foreign key (namespace, name, version, current_commitment_kind, current_commitment_evidence_hash)
    references package_version_evidence(namespace, name, version, kind, evidence_hash),
  add constraint package_versions_status_projection_check
    check (
      (availability_status <> 'active' and status = availability_status)
      or
      (availability_status = 'active' and (
        (current_commitment_evidence_hash is not null and status = 'on_chain_committed')
        or
        (current_commitment_evidence_hash is null and deployment_status in ('deployed', 'chain_verified') and status = 'deployed')
        or
        (current_commitment_evidence_hash is null and deployment_status in ('not_applicable', 'undeployed')
          and verification_status in ('hash_bound', 'verified', 'evidence_required') and status = 'verified_build')
        or
        (current_commitment_evidence_hash is null and deployment_status in ('not_applicable', 'undeployed')
          and verification_status in ('pending', 'rejected') and status in ('source_published', 'indexed_pending'))
      ))
    );
