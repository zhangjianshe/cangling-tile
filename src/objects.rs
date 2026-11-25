//! Private, version-qualified 3D object cache. Never interpret resource keys as disk paths.
use crate::{api::AppState, error::ApiError};
use axum::{
    body::Bytes,
    extract::{Path, Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{delete, get},
    Json, Router,
};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::sync::OnceLock;

pub(crate) fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/objects3d/stats", get(stats))
        .route("/api/v1/objects3d/{dataset}/{revision}", delete(remove))
        .route(
            "/api/v1/objects3d/{dataset}/{revision}/{*path}",
            get(read).put(write),
        )
        .route_layer(axum::middleware::from_fn_with_state(state, authorize))
}
async fn authorize(
    State(s): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    // Unlike public XYZ tiles, even reads require a service credential. Map enforces
    // dataset visibility; direct cache access must not bypass disabled datasets.
    if s.auth_token.is_empty()
        || request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            != Some(format!("Bearer {}", s.auth_token).as_str())
    {
        return Err(ApiError::Unauthorized);
    }
    static LIMIT: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    let _permit = LIMIT
        .get_or_init(|| tokio::sync::Semaphore::new(4))
        .try_acquire()
        .map_err(|_| ApiError::Overloaded)?;
    Ok(next.run(request).await)
}
fn key(dataset: &str, revision: &str, path: &str) -> Result<(), ApiError> {
    if dataset.is_empty()
        || dataset.len() > 128
        || !dataset
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        || revision.len() != 64
        || !revision.bytes().all(|b| b.is_ascii_hexdigit())
        || path.is_empty()
        || path.len() > 2048
        || path
            .chars()
            .any(|c| c.is_control() || matches!(c, '\\' | '?' | '#' | ':'))
        || path
            .split('/')
            .any(|p| p.is_empty() || matches!(p, "." | ".."))
    {
        return Err(ApiError::Invalid("invalid 3D object key".into()));
    }
    Ok(())
}
async fn read(
    State(s): State<AppState>,
    Path((d, r, p)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    key(&d, &r, &p)?;
    let row=sqlx::query("UPDATE objects SET hits=hits+1, accessed=unixepoch() WHERE dataset=? AND revision=? AND path=? RETURNING data, sha256")
        .bind(d).bind(r).bind(p).fetch_optional(s.store.objects().await?).await?.ok_or(ApiError::NotFound)?;
    let data: Vec<u8> = row.get("data");
    let sha: String = row.get("sha256");
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
            (header::ETAG, format!("\"{sha}\"")),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        data,
    )
        .into_response())
}
async fn write(
    State(s): State<AppState>,
    Path((d, r, p)): Path<(String, String, String)>,
    headers: axum::http::HeaderMap,
    data: Bytes,
) -> Result<StatusCode, ApiError> {
    key(&d, &r, &p)?;
    let sha = format!("{:x}", Sha256::digest(&data));
    if data.len() > 32 * 1024 * 1024
        || headers
            .get("x-content-sha256")
            .and_then(|h| h.to_str().ok())
            != Some(&sha)
    {
        return Err(ApiError::Invalid(
            "object too large or SHA-256 mismatch".into(),
        ));
    }
    let pool = s.store.objects().await?;
    // Immutable publication version: a conflicting payload must never replace it.
    sqlx::query("INSERT INTO objects(dataset,revision,path,sha256,data) VALUES(?,?,?,?,?) ON CONFLICT(dataset,revision,path) DO NOTHING")
        .bind(&d).bind(&r).bind(&p).bind(&sha).bind(data.as_ref()).execute(pool).await?;
    let actual: String =
        sqlx::query_scalar("SELECT sha256 FROM objects WHERE dataset=? AND revision=? AND path=?")
            .bind(d)
            .bind(r)
            .bind(p)
            .fetch_one(pool)
            .await?;
    Ok(if actual == sha {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::CONFLICT
    })
}
async fn remove(
    State(s): State<AppState>,
    Path((d, r)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    key(&d, &r, "tileset.json")?;
    sqlx::query("DELETE FROM objects WHERE dataset=? AND revision=?")
        .bind(d)
        .bind(r)
        .execute(s.store.objects().await?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn stats(State(s): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let rows=sqlx::query("SELECT dataset,revision,count(*) AS objects,coalesce(sum(length(data)),0) AS bytes,coalesce(sum(hits),0) AS hits FROM objects GROUP BY dataset,revision ORDER BY dataset,revision")
        .fetch_all(s.store.objects().await?).await?;
    Ok(Json(
        serde_json::json!({"datasets":rows.iter().map(|r|serde_json::json!({"dataset":r.get::<String,_>("dataset"),"revision":r.get::<String,_>("revision"),"objects":r.get::<i64,_>("objects"),"bytes":r.get::<i64,_>("bytes"),"hits":r.get::<i64,_>("hits")})).collect::<Vec<_>>()}),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn authenticated_roundtrip_integrity_versions_and_eviction() {
        use crate::{
            catalog::Catalog,
            model::{CatalogRebuildStatus, CleanupSettings},
            stats::AccessStats,
            store::TileStore,
        };
        use std::sync::{Arc, RwLock};
        use tower::ServiceExt;
        let dir = tempfile::tempdir().unwrap();
        let store = TileStore::open(dir.path(), 1, 8).await.unwrap();
        let config = dir.path().join("config");
        let s = AppState {
            store: store.clone(),
            catalog: Catalog::open(&config, store.clone()).await.unwrap(),
            auth_token: Arc::from("service-secret"),
            stats: AccessStats::open(&config).await.unwrap(),
            cleanup_settings: Arc::new(tokio::sync::RwLock::new(CleanupSettings {
                retention_days: 7,
                cleanup_hour: 4,
                shard_idle_seconds: 300,
            })),
            auth: Arc::new(crate::auth::AuthService::open(&config).await.unwrap()),
            secure_cookies: false,
            http_prefix: Arc::from(""),
            catalog_rebuild: Arc::new(RwLock::new(CatalogRebuildStatus::default())),
        };
        let app = crate::api::router(s, 1024);
        let path = format!("/api/v1/objects3d/demo/{}/nested/a.glb", "a".repeat(64));
        async fn call(
            app: &Router,
            method: &str,
            path: &str,
            auth: bool,
            data: &str,
            sha: &str,
        ) -> Response {
            let mut b = Request::builder()
                .method(method)
                .uri(path)
                .header("x-content-sha256", sha);
            if auth {
                b = b.header(header::AUTHORIZATION, "Bearer service-secret");
            }
            app.clone()
                .oneshot(b.body(axum::body::Body::from(data.to_owned())).unwrap())
                .await
                .unwrap()
        }
        let sha = format!("{:x}", Sha256::digest(b"model"));
        assert_eq!(call(&app, "GET", &path, false, "", "").await.status(), 401);
        assert_eq!(
            call(&app, "PUT", &path, false, "model", &sha)
                .await
                .status(),
            401
        );
        assert_eq!(call(&app, "GET", &path, true, "", "").await.status(), 404);
        assert_eq!(
            call(&app, "PUT", &path, true, "wrong", &sha).await.status(),
            400
        );
        assert_eq!(
            call(&app, "PUT", &path, true, "model", &sha).await.status(),
            204
        );
        let other = format!("{:x}", Sha256::digest(b"other"));
        assert_eq!(
            call(&app, "PUT", &path, true, "other", &other)
                .await
                .status(),
            409
        );
        let response = call(&app, "GET", &path, true, "", "").await;
        assert_eq!(response.status(), 200);
        assert_eq!(
            axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap(),
            "model"
        );
        assert_eq!(
            call(
                &app,
                "GET",
                &path.replace(&"a".repeat(64), &"b".repeat(64)),
                true,
                "",
                ""
            )
            .await
            .status(),
            404
        );
        // The object store is durable, independent of TileStore's process-local state.
        let reopened = TileStore::open(dir.path(), 1, 8).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM objects")
            .fetch_one(reopened.objects().await.unwrap())
            .await
            .unwrap();
        assert_eq!(count, 1);
        let stats = call(&app, "GET", "/api/v1/objects3d/stats", true, "", "").await;
        let json: serde_json::Value =
            serde_json::from_slice(&axum::body::to_bytes(stats.into_body(), 4096).await.unwrap())
                .unwrap();
        assert_eq!(json["datasets"][0]["hits"], 1);
        let revision = path.trim_end_matches("/nested/a.glb");
        assert_eq!(
            call(&app, "DELETE", revision, true, "", "").await.status(),
            204
        );
        assert_eq!(call(&app, "GET", &path, true, "", "").await.status(), 404);
    }
    #[test]
    fn rejects_unsafe_keys() {
        let r = "a".repeat(64);
        for p in ["../a", "/a", "a//b", "a/./b", "a\\b", "a?b", ""] {
            assert!(key("demo", &r, p).is_err());
        }
        assert!(key("demo", &r, "lod 1/模型.glb").is_ok());
        assert!(key("../demo", &r, "a").is_err());
    }
}
