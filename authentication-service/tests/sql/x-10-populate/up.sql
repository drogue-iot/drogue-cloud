--
-- tenant1
--

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

--
-- device1 -> pass: foo
--

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

--
-- device2 -> must not exist
--

--
-- device3 (aka: 12:34:56) -> user: foo/bar
--

INSERT INTO DEVICES (
    TENANT_ID,
    ID,
    DATA
) VALUES (
    'tenant1',
    'device3',
    '{
       "credentials": [
         { "user": { "username": "device3", "password": "baz"}},
         { "user": { "username": "foo", "password": "bar" }}
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
    'device3',
    'id',
    'device3'
);

INSERT INTO DEVICE_ALIASES(
    TENANT_ID,
    ID,
    TYPE,
    ALIAS
) VALUES (
    'tenant1',
    'device3',
    'hwaddr',
    '12:34:56'
);
