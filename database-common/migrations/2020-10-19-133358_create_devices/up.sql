CREATE TABLE credentials (
    device_id VARCHAR PRIMARY KEY,
    secret JSONB,
    properties JSONB
);
