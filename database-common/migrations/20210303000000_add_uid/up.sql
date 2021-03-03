
-- shuffle columns

-- for application

ALTER TABLE applications
    RENAME COLUMN ID TO NAME
;

ALTER TABLE applications
    ADD COLUMN UID uuid NOT NULL DEFAULT gen_random_uuid()
;

ALTER TABLE application_aliases
    RENAME COLUMN ID TO APP
;

ALTER TABLE application_aliases
    RENAME CONSTRAINT application_aliases_id_fkey TO application_aliases_app_fkey
;

-- for: devices

ALTER TABLE devices
    RENAME COLUMN ID TO NAME
;

ALTER TABLE devices
    ADD COLUMN UID uuid NOT NULL DEFAULT gen_random_uuid()
;

ALTER TABLE devices
    RENAME COLUMN APP_ID TO APP
;

ALTER TABLE devices
    RENAME CONSTRAINT devices_app_id_fkey TO devices_app_fkey
;

ALTER TABLE device_aliases
    RENAME COLUMN ID TO DEVICE
;

ALTER TABLE device_aliases
    RENAME COLUMN APP_ID TO APP
;

ALTER TABLE device_aliases
    RENAME CONSTRAINT device_aliases_app_id_fkey TO device_aliases_app_fkey
;

ALTER TABLE device_aliases
    RENAME CONSTRAINT device_aliases_id_app_id_fkey TO device_aliases_device_app_fkey
;
