CREATE TABLE credentials (
    device_id VARCHAR PRIMARY KEY,
    secret_type INTEGER NOT NULL,
    secret JSONB
);

INSERT INTO credentials(device_id, secret_type, secret) VALUES
('device1', '1', '{"hash": "2188dd0b20077359488b272f485d90dc1267f212b2d9e23e46a281161b54ae3f", "salt": "alongFixedSizeSaltString"}');