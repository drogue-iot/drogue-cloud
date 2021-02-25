--
-- app1
--

INSERT INTO APPLICATIONS (
    ID,
    CREATION_TIMESTAMP,
    RESOURCE_VERSION,
    GENERATION,
    DATA
) VALUES (
    'app1',
    now(),
    gen_random_uuid(),
    0,
    '{}'::JSONB
);

INSERT INTO APPLICATION_ALIASES (
    ID,
    TYPE,
    ALIAS
) VALUES (
    'app1',
    'id',
    'app1'
);

--
-- device1 -> pass: foo
--

INSERT INTO DEVICES (
    APP_ID,
    ID,
    CREATION_TIMESTAMP,
    RESOURCE_VERSION,
    GENERATION,
    DATA
) VALUES (
    'app1',
    'device1',
    now(),
    gen_random_uuid(),
    0,
    '{
      "spec": {
        "credentials": {
          "credentials": [
            { "pass": "foo"}
          ]
        }
      }
    }'::JSONB
);

INSERT INTO DEVICE_ALIASES(
    APP_ID,
    ID,
    TYPE,
    ALIAS
) VALUES (
    'app1',
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
    APP_ID,
    ID,
    CREATION_TIMESTAMP,
    RESOURCE_VERSION,
    GENERATION,
    DATA
) VALUES (
    'app1',
    'device3',
    now(),
    gen_random_uuid(),
    0,
    '{
       "spec": {
         "credentials": {
           "credentials": [
             { "user": { "username": "device3", "password": "baz"}},
             { "user": { "username": "foo", "password": "bar" }}
           ]
         }
       }
     }'::JSONB
);

INSERT INTO DEVICE_ALIASES(
    APP_ID,
    ID,
    TYPE,
    ALIAS
) VALUES (
    'app1',
    'device3',
    'id',
    'device3'
);

INSERT INTO DEVICE_ALIASES(
    APP_ID,
    ID,
    TYPE,
    ALIAS
) VALUES (
    'app1',
    'device3',
    'hwaddr',
    '12:34:56'
);
