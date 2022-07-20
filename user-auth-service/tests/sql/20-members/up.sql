--
-- app-member1
--

INSERT INTO APPLICATIONS (
    NAME,
    UID,
    CREATION_TIMESTAMP,
    RESOURCE_VERSION,
    GENERATION,
    REVISION,
    OWNER,
    MEMBERS,
    DATA
) VALUES (
    'app-member1',
    'a54d393c-bd6f-11eb-ad45-d45d6455d2cc',
    '2020-01-01 00:00:00',
    'ab2cefd2-bd6f-11eb-9487-d45d6455d2cc',
    0,
    0,
    'foo',
    '{
        "bar-admin": { "role": "admin" },
        "bar-manager": { "role": "manager" },
        "bar-reader": { "role": "reader" },
        "bar-publisher": { "role": "publisher" },
        "": { "role": "reader" }
     }'::JSONB,
    '{}'::JSONB
);

INSERT INTO APPLICATION_ALIASES (
    APP,
    TYPE,
    ALIAS
) VALUES (
     'app-member1',
     'id',
     'app-member1'
 );
