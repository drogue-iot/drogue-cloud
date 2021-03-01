CREATE TABLE outbox (
    -- the instance ID
    -- we carry this information along, but to not enforce uniqueness here, as we expect every
    -- instance to have its own database
    INSTANCE_ID VARCHAR(255) NOT NULL,

    APP_ID VARCHAR(64) NOT NULL,
    DEVICE_ID VARCHAR(256) NOT NULL,
    PATH VARCHAR(255) NOT NULL,

    GENERATION BIGINT NOT NULL,
    TS TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),

    PRIMARY KEY (APP_ID, DEVICE_ID, PATH)
);

-- create an index to make it easier to delete entries by and ID and generation

CREATE INDEX OUTBOX_BY_IDS_AND_GEN ON outbox (
    APP_ID,
    DEVICE_ID,
    PATH,

    GENERATION
);

-- create an index to make it easier to find entries by ID and timestamp

CREATE INDEX OUTBOX_BY_IDS_AND_TIMESTAMP ON outbox (
    APP_ID,
    DEVICE_ID,
    PATH,

    TS
);
