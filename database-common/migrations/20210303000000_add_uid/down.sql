
-- shuffle columns

-- for application

ALTER TABLE applications
    RENAME COLUMN NAME TO ID
;

ALTER TABLE applications
    DROP COLUMN UID
;

ALTER TABLE application_aliases
    RENAME COLUMN APP TO ID
;

ALTER TABLE application_aliases
    RENAME CONSTRAINT application_aliases_app_fkey TO application_aliases_id_fkey
;

-- for: devices

ALTER TABLE devices
    RENAME COLUMN NAME TO ID
;

ALTER TABLE devices
    DROP COLUMN UID
;

ALTER TABLE devices
    RENAME COLUMN APP TO APP_ID
;

ALTER TABLE devices
    RENAME CONSTRAINT devices_app_fkey TO devices_app_id_fkey
;

ALTER TABLE device_aliases
    RENAME COLUMN DEVICE TO ID
;

ALTER TABLE device_aliases
    RENAME COLUMN APP TO APP_ID
;

ALTER TABLE device_aliases
    RENAME CONSTRAINT device_aliases_app_fkey TO device_aliases_app_id_fkey
;

ALTER TABLE device_aliases
    RENAME CONSTRAINT device_aliases_device_app_fkey TO device_aliases_id_app_id_fkey
;
