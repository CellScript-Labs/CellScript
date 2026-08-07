alter table principals
  drop constraint if exists principals_principal_type_check;

alter table principals
  add constraint principals_principal_type_check
  check (principal_type in ('joyid_ckb', 'ckb_secp256k1'));

comment on column capabilities.joyid_signature is
  'Legacy column name; stores the verified root-wallet signature envelope for joyid_ckb or ckb_secp256k1 principals.';
