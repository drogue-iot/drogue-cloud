ALTER TABLE credentials ADD properties JSONB;

UPDATE credentials
SET properties = '{"model":"stm32"}'
WHERE device_id = 'device1';