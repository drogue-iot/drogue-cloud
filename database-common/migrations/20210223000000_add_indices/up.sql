-- creating an index for efficiently accessing devices by application id
CREATE INDEX DEVICES_BY_APP_ID_IDX ON DEVICES (APP_ID);
CREATE INDEX DEVICES_BY_FINALIZER_COUNT ON DEVICES(APP_ID, (array_length ( FINALIZERS, 1 ) = 0));
