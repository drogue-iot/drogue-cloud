-- create 5.000.000 outbox entries for testing
INSERT INTO outbox (instance_id, app_id, device_id, path, generation, ts)
SELECT 'drogue', i::text, '', '.', 0, now() FROM generate_series(1, 5000000) as t(i);