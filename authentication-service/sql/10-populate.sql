INSERT INTO TENANTS (
    ID,
    DATA
) VALUES (
    'tenant1',
    '{}'::JSONB
);

INSERT INTO TENANT_ALIASES (
    ID,
    TYPE,
    ALIAS
) VALUES (
    'tenant1',
    'id',
    'tenant1'
);

INSERT INTO DEVICES (
    TENANT_ID,
    ID,
    DATA
) VALUES (
    'tenant1',
    'device1',
    '{
      "credentials": [
        { "pass": "foo"}
      ]
    }'::JSONB
);

INSERT INTO DEVICE_ALIASES(
    TENANT_ID,
    ID,
    TYPE,
    ALIAS
) VALUES (
    'tenant1',
    'device1',
    'id',
    'device1'
);
