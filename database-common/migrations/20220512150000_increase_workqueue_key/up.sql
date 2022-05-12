-- increase to hold at least application plus device name plus UID, plus some extra
ALTER TABLE WORKQUEUE
    ALTER COLUMN KEY TYPE VARCHAR(512);