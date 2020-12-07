diesel::table! {
    credentials (device_id) {
        device_id -> Varchar,
        secret -> Nullable<Jsonb>,
        properties -> Nullable<Jsonb>,
    }
}
