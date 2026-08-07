alter table package_version_evidence
  drop constraint if exists package_version_evidence_kind_check;

alter table package_version_evidence
  add constraint package_version_evidence_kind_check
    check (kind in ('verified_build', 'reproduced_build', 'deployed', 'on_chain_attested'));
