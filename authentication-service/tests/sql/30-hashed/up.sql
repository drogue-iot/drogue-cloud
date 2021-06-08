--
-- app3
--

INSERT INTO APPLICATIONS (
    NAME,
    UID,
    CREATION_TIMESTAMP,
    RESOURCE_VERSION,
    GENERATION,
    DATA
) VALUES (
    'app3',
    '4cf9607e-c7ad-11eb-8d69-d45d6455d2cc',
    '2021-01-01 00:00:00',
    '547531d4-c7ad-11eb-abee-d45d6455d2cc',
    0,
    '{}'::JSONB
);

INSERT INTO APPLICATION_ALIASES (
    APP,
    TYPE,
    ALIAS
) VALUES (
    'app3',
    'id',
    'app3'
);

--
-- device1 -> pass: plain(foo)
--

INSERT INTO DEVICES (
    APP,
    NAME,
    UID,
    CREATION_TIMESTAMP,
    RESOURCE_VERSION,
    GENERATION,
    DATA
) VALUES (
    'app3',
    'device1',
    '4e185ea6-7c26-11eb-a319-d45d6455d211',
    '2020-01-01 00:00:00',
    'A0EEBC99-9C0B-4EF8-BB6D-6BB9BD380A11',
    0,
    '{
      "spec": {
        "credentials": {
          "credentials": [
            { "pass": { "plain": "foo" } }
          ]
        }
      }
    }'::JSONB
);

INSERT INTO DEVICE_ALIASES(
    APP,
    DEVICE,
    TYPE,
    ALIAS
) VALUES (
    'app3',
    'device1',
    'id',
    'device1'
);


--
-- device2 -> pass: sha512(foo)
--

INSERT INTO DEVICES (
    APP,
    NAME,
    UID,
    CREATION_TIMESTAMP,
    RESOURCE_VERSION,
    GENERATION,
    DATA
) VALUES (
    'app3',
    'device2',
    '8bcfeb78-c7ae-11eb-9535-d45d6455d2cc',
    '2020-01-01 00:00:00',
    'A0EEBC99-9C0B-4EF8-BB6D-6BB9BD380A11',
    0,
    '{
    "spec": {
     "credentials": {
       "credentials": [
         { "pass": { "bcrypt": "$2y$12$fR.P62Obq5BzezX3i6AmdO1.m2uj44PutU8mejlK.2MDEpGcxU7w." } }
       ]
     }
    }
    }'::JSONB
);

INSERT INTO DEVICE_ALIASES(
    APP,
    DEVICE,
    TYPE,
    ALIAS
) VALUES (
    'app3',
    'device2',
    'id',
    'device2'
);


--
-- device3 -> pass: bcrypt(foo)
--

INSERT INTO DEVICES (
    APP,
    NAME,
    UID,
    CREATION_TIMESTAMP,
    RESOURCE_VERSION,
    GENERATION,
    DATA
) VALUES (
    'app3',
    'device3',
    '91023af6-c7ae-11eb-9902-d45d6455d2cc',
    '2020-01-01 00:00:00',
    'A0EEBC99-9C0B-4EF8-BB6D-6BB9BD380A11',
    0,
    '{
    "spec": {
     "credentials": {
       "credentials": [
         { "pass": { "sha512": "$6$edo9OjTnrWxXUtZm$SLCWIX9Mecm.4NrI9V9Jh6h9kGrruBp/Bu1U8ACJgDDDN1GWofPd9gbp80X/1JZkFLgZoIMVy9q4sdPLbmSp//" } }
       ]
     }
    }
    }'::JSONB
);

INSERT INTO DEVICE_ALIASES(
    APP,
    DEVICE,
    TYPE,
    ALIAS
) VALUES (
    'app3',
    'device3',
    'id',
    'device3'
);