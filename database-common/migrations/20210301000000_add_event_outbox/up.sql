CREATE TABLE outbox (
    -- the instance ID
    -- we carry this information along, but to not enforce uniqueness here, as we expect every
    -- instance to have its own database
    INSTANCE VARCHAR(255) NOT NULL,

    APP VARCHAR(64) NOT NULL,
    DEVICE VARCHAR(256) NOT NULL,
    PATH VARCHAR(255) NOT NULL,

    GENERATION BIGINT NOT NULL,
    UID VARCHAR(64) NOT NULL,
    TS TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),

    PRIMARY KEY (APP, DEVICE, PATH)
);

-- create an index to make it easier to find entries by timestamp

CREATE INDEX OUTBOX_TS ON outbox (
    TS ASC
);
