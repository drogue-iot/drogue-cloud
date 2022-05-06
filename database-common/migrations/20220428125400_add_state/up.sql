CREATE TABLE sessions
(
    ID        UUID                     NOT NULL,
    LAST_PING TIMESTAMP WITH TIME ZONE NOT NULL,

    PRIMARY KEY (ID)
);

CREATE TABLE states
(
    APPLICATION VARCHAR(64)              NOT NULL,
    DEVICE      VARCHAR(255)             NOT NULL,
    SESSION     UUID                     NOT NULL,
    TOKEN       VARCHAR(64)              NOT NULL,

    CREATED     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    LOST        BOOLEAN                  NOT NULL DEFAULT false,

    DATA        JSONB,

    PRIMARY KEY (APPLICATION, DEVICE),
    FOREIGN KEY (SESSION) REFERENCES sessions (ID)
);