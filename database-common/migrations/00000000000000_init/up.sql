CREATE TABLE applications
(
    ID      VARCHAR(64) NOT NULL,
    LABELS  JSONB,

    DATA    JSONB,

    PRIMARY KEY (ID)
);

CREATE TABLE application_aliases
(
    TYPE    VARCHAR(32)  NOT NULL, -- type of the alias, e.g. 'id'
    ALIAS   VARCHAR(256) NOT NULL, -- value of the alias, e.g. <id>
    ID      VARCHAR(64)  NOT NULL,

    PRIMARY KEY (ALIAS),
    FOREIGN KEY (ID) REFERENCES applications (ID) ON DELETE CASCADE
);

CREATE TABLE devices
(
    ID          VARCHAR(256) NOT NULL,
    APP_ID      VARCHAR(64)  NOT NULL,
    LABELS      JSONB,

    DATA        JSONB,

    PRIMARY KEY (ID, APP_ID),
    FOREIGN KEY (APP_ID) REFERENCES applications (ID) ON DELETE CASCADE
);

CREATE TABLE device_aliases
(
    TYPE    VARCHAR(32)  NOT NULL, -- type of the alias, e.g. 'id'
    ALIAS   VARCHAR(256) NOT NULL, -- value of the alias, e.g. <id>
    ID      VARCHAR(256) NOT NULL,
    APP_ID  VARCHAR(64)  NOT NULL,

    PRIMARY KEY (ALIAS, APP_ID),
    FOREIGN KEY (ID, APP_ID) REFERENCES devices (ID, APP_ID) ON DELETE CASCADE,
    FOREIGN KEY (APP_ID) REFERENCES applications (ID) ON DELETE CASCADE
);
