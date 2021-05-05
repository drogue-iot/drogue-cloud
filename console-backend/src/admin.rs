use actix_web::HttpResponse;
use drogue_cloud_service_api::auth::user::UserInformation;
use serde_json::json;

pub async fn whoami(user: UserInformation) -> Result<HttpResponse, actix_web::Error> {
    Ok(HttpResponse::Ok().json(json!({
        "id": user.user_id(),
    })))
}
