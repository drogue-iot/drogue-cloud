diesel::table! {
    credentials (device_id) {
        device_id -> Varchar,
        secret_type -> Int4,
        secret -> Nullable<Text>,
    }
}
