-- Account egress is separate from provider credentials and scheduling groups.
CREATE TABLE egress_proxies (
    id text PRIMARY KEY,
    name text NOT NULL CHECK (length(name) BETWEEN 1 AND 100),
    endpoint text NOT NULL,
    enabled boolean NOT NULL DEFAULT true,
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE account_egress_bindings (
    account_id text PRIMARY KEY REFERENCES provider_accounts(id) ON DELETE CASCADE,
    proxy_id text NOT NULL REFERENCES egress_proxies(id) ON DELETE RESTRICT,
    generation text NOT NULL
);
CREATE INDEX account_egress_proxy_idx ON account_egress_bindings(proxy_id);
