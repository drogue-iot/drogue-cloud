ALTER TABLE devices
    DROP COLUMN IF EXISTS ANNOTATIONS,
    DROP COLUMN IF EXISTS DELETION_TIMESTAMP,
    DROP COLUMN IF EXISTS FINALIZERS,
    DROP COLUMN IF EXISTS CREATION_TIMESTAMP,
    DROP COLUMN IF EXISTS RESOURCE_VERSION,
    DROP COLUMN IF EXISTS GENERATION
;

ALTER TABLE application
    DROP COLUMN IF EXISTS ANNOTATIONS,
    DROP COLUMN IF EXISTS DELETION_TIMESTAMP,
    DROP COLUMN IF EXISTS FINALIZERS,
    DROP COLUMN IF EXISTS CREATION_TIMESTAMP,
    DROP COLUMN IF EXISTS RESOURCE_VERSION,
    DROP COLUMN IF EXISTS GENERATION
;
