CREATE TABLE command_sessions
(
    ID          UUID                     NOT NULL,
    LAST_PING   TIMESTAMP WITH TIME ZONE NOT NULL,
    SESSION_URL VARCHAR(255)             NOT NULL,

    PRIMARY KEY (ID)
);

CREATE TABLE command_routes
(
    APPLICATION VARCHAR(64)              NOT NULL,
    DEVICE      VARCHAR(255)             NOT NULL,
    COMMAND     VARCHAR(255)             NOT NULL,

    SESSION     UUID                     NOT NULL,

    PRIMARY KEY (APPLICATION, DEVICE, COMMAND, SESSION),
    FOREIGN KEY (SESSION) REFERENCES command_sessions (ID)
);
