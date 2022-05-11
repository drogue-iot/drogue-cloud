--
-- app1
--

INSERT INTO APPLICATIONS (
    NAME,
    UID,
    CREATION_TIMESTAMP,
    RESOURCE_VERSION,
    GENERATION,
    REVISION,
    OWNER,
    DATA
) VALUES (
    'app1',
    '4e185ea6-7c26-11eb-a319-d45d6455d210',
    '2020-01-01 00:00:00',
    'A0EEBC99-9C0B-4EF8-BB6D-6BB9BD380A11',
    0,
    0,
    'user1',
    '{}'::JSONB
);

INSERT INTO APPLICATION_ALIASES (
    APP,
    TYPE,
    ALIAS
) VALUES (
    'app1',
    'name',
    'app1'
);
