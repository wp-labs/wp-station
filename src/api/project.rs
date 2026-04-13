use actix_web::{HttpRequest, HttpResponse, post};
use urlencoding::decode;

use crate::error::AppError;
use crate::server::project::import_project_from_files_logic;

fn operator_from_request(req: &HttpRequest) -> Option<String> {
    req.headers().get("x-operator").and_then(|value| {
        let raw = value.to_str().ok()?.trim();
        if raw.is_empty() {
            return None;
        }
        decode(raw)
            .ok()
            .map(|cow| cow.trim().to_string())
            .filter(|decoded| !decoded.is_empty())
    })
}

#[post("/api/project/import")]
pub async fn import_project_from_files(http_req: HttpRequest) -> Result<HttpResponse, AppError> {
    let operator = operator_from_request(&http_req);
    let resp = import_project_from_files_logic(operator).await?;
    Ok(HttpResponse::Ok().json(resp))
}
