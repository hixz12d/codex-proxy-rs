alter table runtime_settings
  add column account_fingerprint_enabled boolean not null default true;
