alter table package_versions
  drop constraint if exists package_versions_verification_status_check;

alter table package_versions
  add constraint package_versions_verification_status_check
    check (verification_status in ('pending', 'hash_bound', 'verified', 'evidence_required', 'rejected'));
