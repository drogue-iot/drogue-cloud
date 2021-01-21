CREATE TABLE tenants
(
    ID   VARCHAR(64) NOT NULL,

    DATA JSONB,

    PRIMARY KEY (ID)
);

CREATE TABLE tenant_aliases
(
    TYPE  VARCHAR(32)  NOT NULL, -- type of the alias, e.g. 'id'
    ALIAS VARCHAR(256) NOT NULL, -- value of the alias, e.g. <id>
    ID    VARCHAR(64)  NOT NULL,

    PRIMARY KEY (ALIAS),
    FOREIGN KEY (ID) REFERENCES tenants (ID) ON DELETE CASCADE
);

CREATE TABLE devices
(
    ID        VARCHAR(256) NOT NULL,
    TENANT_ID VARCHAR(64)  NOT NULL,

    DATA      JSONB,

    PRIMARY KEY (ID, TENANT_ID),
    FOREIGN KEY (TENANT_ID) REFERENCES tenants (ID) ON DELETE CASCADE
);

CREATE TABLE device_aliases
(
    TYPE      VARCHAR(32)  NOT NULL, -- type of the alias, e.g. 'id'
    ALIAS     VARCHAR(256) NOT NULL, -- value of the alias, e.g. <id>
    ID        VARCHAR(256) NOT NULL,
    TENANT_ID VARCHAR(64)  NOT NULL,

    PRIMARY KEY (ALIAS, TENANT_ID),
    FOREIGN KEY (ID, TENANT_ID) REFERENCES devices (ID, TENANT_ID) ON DELETE CASCADE,
    FOREIGN KEY (TENANT_ID) REFERENCES tenants (ID) ON DELETE CASCADE
);
