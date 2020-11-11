use diesel::Queryable;

#[derive(Queryable)]
pub struct Credential {
    pub device_id: String,
    pub secret_type: i32,
    pub secret: Option<String>,
}
