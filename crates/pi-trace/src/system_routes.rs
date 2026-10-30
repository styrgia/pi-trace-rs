use crate::{
    catalog::Catalog,
    service::{ApiError, AppState, api_error, no_query, parameters},
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, RawQuery, Request, State, rejection::PathRejection},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::sync::Arc;
use trace_core::native::{NativeError, Provider};
struct Error(NativeError);
impl From<NativeError> for Error {
    fn from(e: NativeError) -> Self {
        Self(e)
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match self.0.code.as_str() {
            "unknown_session" | "unknown_snapshot" | "unknown_node" | "unknown_span" => {
                StatusCode::NOT_FOUND
            }
            "analysis_capacity" => StatusCode::SERVICE_UNAVAILABLE,
            "analysis_busy" | "expired_catalog_revision" => StatusCode::CONFLICT,
            "view_limit" => StatusCode::PAYLOAD_TOO_LARGE,
            _ => StatusCode::BAD_REQUEST,
        };
        let capacity = self.0.code == "analysis_capacity";
        let mut response = (
            status,
            Json(json!({"error":self.0,"retryAfterMs":if capacity{Some(2000)}else{None}})),
        )
            .into_response();
        if capacity {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, header::HeaderValue::from_static("2"));
        }
        response
    }
}
fn catalog(state: &AppState) -> Result<&Arc<Catalog>, ApiError> {
    state.catalog.as_ref().ok_or_else(|| {
        api_error(
            StatusCode::CONFLICT,
            "mode_conflict",
            "System routes require system mode.",
        )
    })
}
fn path<T>(path: Result<Path<T>, PathRejection>) -> Result<T, ApiError> {
    path.map(|Path(v)| v).map_err(|_| {
        api_error(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            "The API path is invalid.",
        )
    })
}
async fn empty(request: Request) -> Result<(), ApiError> {
    to_bytes(request.into_body(), 0).await.map_err(|_| {
        api_error(
            StatusCode::BAD_REQUEST,
            "unexpected_payload",
            "System routes accept no request body.",
        )
    })?;
    Ok(())
}
fn bounded(value: Value) -> Response {
    match serde_json::to_vec(&value) {
        Ok(bytes) if bytes.len() <= 1024 * 1024 => Json(value).into_response(),
        _ => api_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "view_limit",
            "The view exceeds the response limit.",
        )
        .into_response(),
    }
}
async fn status(
    State(state): State<Arc<AppState>>,
    RawQuery(query): RawQuery,
    request: Request,
) -> Result<Response, ApiError> {
    no_query(query)?;
    empty(request).await?;
    Ok(bounded(catalog(&state)?.status()))
}
async fn sessions(
    State(state): State<Arc<AppState>>,
    RawQuery(raw): RawQuery,
    request: Request,
) -> Result<Response, ApiError> {
    empty(request).await?;
    let raw = raw.unwrap_or_default();
    let invalid = || {
        api_error(
            StatusCode::BAD_REQUEST,
            "invalid_query",
            "The catalog query is unsupported, repeated or invalid.",
        )
    };
    if raw.len() > 16384 {
        return Err(invalid());
    }
    let mut provider = None;
    let mut project = None;
    let mut cursor = None;
    let mut limit = 100;
    let mut seen = std::collections::HashSet::new();
    for (k, v) in form_urlencoded::parse(raw.as_bytes()) {
        if !seen.insert(k.to_string()) {
            return Err(invalid());
        }
        match k.as_ref() {
            "provider" => {
                v.parse::<Provider>().map_err(|_| invalid())?;
                provider = Some(v.into_owned());
            }
            "project" if v.len() <= 512 => project = Some(v.into_owned()),
            "cursor" if v.len() <= 1024 => cursor = Some(v.into_owned()),
            "limit" => limit = v.parse().map_err(|_| invalid())?,
            _ => return Err(invalid()),
        }
    }
    Ok(
        match catalog(&state)?.page(
            provider.as_deref(),
            project.as_deref(),
            cursor.as_deref(),
            limit,
        ) {
            Ok(v) => bounded(v),
            Err(e) => Error(e).into_response(),
        },
    )
}
async fn workspace(
    State(state): State<Arc<AppState>>,
    p: Result<Path<String>, PathRejection>,
    RawQuery(query): RawQuery,
    request: Request,
) -> Result<Response, ApiError> {
    no_query(query)?;
    empty(request).await?;
    let id = path(p)?;
    Ok(match catalog(&state)?.workspace(&id) {
        Ok(v) => bounded(json!(v)),
        Err(e) => Error(e).into_response(),
    })
}
async fn retry(
    State(state): State<Arc<AppState>>,
    p: Result<Path<String>, PathRejection>,
    RawQuery(query): RawQuery,
    request: Request,
) -> Result<Response, ApiError> {
    no_query(query)?;
    empty(request).await?;
    let id = path(p)?;
    Ok(match catalog(&state)?.retry(&id) {
        Ok(v) => (StatusCode::ACCEPTED, Json(v)).into_response(),
        Err(e) => Error(e).into_response(),
    })
}
async fn view(
    State(state): State<Arc<AppState>>,
    p: Result<Path<(String, String, String)>, PathRejection>,
    RawQuery(raw): RawQuery,
    request: Request,
) -> Result<Response, ApiError> {
    empty(request).await?;
    let (session, snapshot, view) = path(p)?;
    let segments: Vec<_> = view.split('/').collect();
    let (observations, diagnostics) = match segments.as_slice() {
        ["observations"] => parameters(raw, true)?,
        ["diagnostics"] => parameters(raw, false)?,
        ["overview"] | ["nodes", _] | ["spans", _] | ["nodes", _, "content"] => {
            no_query(raw)?;
            Default::default()
        }
        _ => {
            return Err(api_error(
                StatusCode::NOT_FOUND,
                "unknown_api_route",
                "The session view is unavailable.",
            ));
        }
    };
    let snapshot = match catalog(&state)?.snapshot(&session, &snapshot) {
        Ok(s) => s,
        Err(e) => return Ok(Error(e).into_response()),
    };
    let result = match segments.as_slice() {
        ["overview"] => snapshot.overview().map(|v| json!(v)),
        ["observations"] => snapshot.observations(&observations).map(|v| json!(v)),
        ["diagnostics"] => snapshot.diagnostics(&diagnostics).map(|v| json!(v)),
        ["nodes", id] => snapshot.node(id).map(|v| json!(v)),
        ["spans", id] => snapshot.span(id).map(|v| json!(v)),
        ["nodes", id, "content"] => snapshot.content(id).map(|v| json!(v)),
        _ => unreachable!(),
    };
    Ok(match result {
        Ok(v) => bounded(v),
        Err(e) => Error(e).into_response(),
    })
}
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/system", get(status))
        .route("/api/v1/sessions", get(sessions))
        .route("/api/v1/sessions/{session}/workspace", get(workspace))
        .route("/api/v1/sessions/{session}/retry", post(retry))
        .route(
            "/api/v1/sessions/{session}/snapshots/{snapshot}/{*view}",
            get(view),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        service,
        system_config::{StoreConfig, SystemConfig},
    };
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    async fn request(
        app: &Router,
        method: &str,
        url: &str,
        origin: Option<&str>,
        body: &str,
    ) -> (StatusCode, Value) {
        let mut r = Request::builder()
            .method(method)
            .uri(url)
            .header("host", "127.0.0.1:4317");
        if let Some(origin) = origin {
            r = r.header("origin", origin);
        }
        let response = app
            .clone()
            .oneshot(r.body(Body::from(body.to_owned())).unwrap())
            .await
            .unwrap();
        assert_eq!(response.headers()["cache-control"], "no-store");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    fn fixture() -> (tempfile::TempDir, Arc<AppState>, Router) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("example.jsonl"),concat!("{\"type\":\"session\",\"version\":3,\"id\":\"fabricated-session\",\"cwd\":\"/fabricated/project\"}\n","{\"type\":\"message\",\"id\":\"m1\",\"message\":{\"role\":\"assistant\",\"content\":\"UNIQUE_PRIVATE_BODY_812\",\"usage\":{\"input\":2,\"output\":3}}}\n")).unwrap();
        let state = AppState::system(SystemConfig {
            stores: vec![StoreConfig {
                provider: Provider::Pi,
                path: dir.path().to_owned(),
            }],
            disabled: vec![Provider::Codex, Provider::Claude, Provider::Opencode],
        });
        state.catalog.as_ref().unwrap().test_discover();
        let router = service::router(state.clone());
        (dir, state, router)
    }
    async fn session(app: &Router) -> String {
        for _ in 0..200 {
            let (_, v) = request(app, "GET", "/api/v1/sessions", None, "").await;
            if let Some(id) = v["items"][0]["id"].as_str() {
                return id.into();
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("catalog did not discover fixture")
    }
    fn contract(
        name: &str,
        value: &Value,
        identities: &std::collections::BTreeMap<String, String>,
    ) {
        fn normalize(value: &mut Value, ids: &std::collections::BTreeMap<String, String>) {
            match value {
                Value::String(s) => {
                    if let Some(replacement) = ids.get(s) {
                        *s = replacement.clone();
                    }
                }
                Value::Array(values) => {
                    for value in values {
                        normalize(value, ids);
                    }
                }
                Value::Object(values) => {
                    for (key, value) in values {
                        if key == "root" {
                            *value = json!("/synthetic/store");
                        } else if matches!(key.as_str(), "activityMs" | "lastObservedMs")
                            && value.is_number()
                        {
                            *value = json!(1700000000000u64);
                        } else {
                            normalize(value, ids);
                        }
                    }
                }
                _ => {}
            }
        }
        let mut value = value.clone();
        normalize(&mut value, identities);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/contracts");
        let path = root.join(format!("system-{name}.json"));
        let serialized = format!("{}\n", serde_json::to_string_pretty(&value).unwrap());
        if std::env::var_os("UPDATE_CONTRACTS").is_some() {
            std::fs::create_dir_all(root).unwrap();
            std::fs::write(&path, &serialized).unwrap();
        }
        assert_eq!(std::fs::read_to_string(path).unwrap(), serialized);
    }
    #[tokio::test]
    async fn actual_serialized_contracts_and_private_content_boundary() {
        let (dir, state, app) = fixture();
        let id = session(&app).await;
        let (_, loading) = request(
            &app,
            "GET",
            &format!("/api/v1/sessions/{id}/workspace"),
            None,
            "",
        )
        .await;
        state.catalog.as_ref().unwrap().test_parse();
        let (_, workspace) = request(
            &app,
            "GET",
            &format!("/api/v1/sessions/{id}/workspace"),
            None,
            "",
        )
        .await;
        let snapshot = workspace["snapshotId"].as_str().unwrap();
        let (_, status) = request(&app, "GET", "/api/v1/system", None, "").await;
        assert_eq!(status["stores"][0]["root"], json!(dir.path()));
        let (_, catalog) = request(&app, "GET", "/api/v1/sessions", None, "").await;
        let (_, rows) = request(
            &app,
            "GET",
            &format!("/api/v1/sessions/{id}/snapshots/{snapshot}/observations"),
            None,
            "",
        )
        .await;
        let mut ids = std::collections::BTreeMap::new();
        ids.insert(id.clone(), "synthetic-session".into());
        ids.insert(
            status["stores"][0]["id"].as_str().unwrap().into(),
            "synthetic-store".into(),
        );
        ids.insert(
            workspace["sourceRevision"].as_str().unwrap().into(),
            "synthetic-source-revision".into(),
        );
        ids.insert(
            workspace["runId"].as_str().unwrap().into(),
            "synthetic-run".into(),
        );
        ids.insert(snapshot.into(), "synthetic-snapshot".into());
        for (i, row) in rows["items"].as_array().unwrap().iter().enumerate() {
            ids.insert(
                row["id"].as_str().unwrap().into(),
                format!("synthetic-node-{i}"),
            );
            ids.insert(
                row["spanId"].as_str().unwrap().into(),
                format!("synthetic-span-{i}"),
            );
        }
        contract("status", &status, &ids);
        contract("catalog", &catalog, &ids);
        contract("workspace-loading", &loading, &ids);
        contract("workspace-ready", &workspace, &ids);
        for view in ["overview", "observations", "diagnostics"] {
            let (code, v) = request(
                &app,
                "GET",
                &format!("/api/v1/sessions/{id}/snapshots/{snapshot}/{view}"),
                None,
                "",
            )
            .await;
            assert_eq!(code, StatusCode::OK);
            assert!(!v.to_string().contains("UNIQUE_PRIVATE_BODY_812"));
            contract(view, &v, &ids);
        }
        assert!(!status.to_string().contains("UNIQUE_PRIVATE_BODY_812"));
        assert!(!catalog.to_string().contains("UNIQUE_PRIVATE_BODY_812"));
        let (_, err) = request(
            &app,
            "GET",
            &format!("/api/v1/sessions/{id}/snapshots/forged/overview"),
            None,
            "",
        )
        .await;
        assert_eq!(err["error"]["code"], "unknown_snapshot");
        contract("error", &err, &ids);
        let native_node = rows["items"][1]["id"].as_str().unwrap();
        let native_span = rows["items"][1]["spanId"].as_str().unwrap();
        for (name, path) in [
            ("node", format!("nodes/{native_node}")),
            ("span", format!("spans/{native_span}")),
            ("content", format!("nodes/{native_node}/content")),
        ] {
            let (status, value) = request(
                &app,
                "GET",
                &format!("/api/v1/sessions/{id}/snapshots/{snapshot}/{path}"),
                None,
                "",
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            if name == "content" {
                assert!(value.to_string().contains("UNIQUE_PRIVATE_BODY_812"));
            } else {
                assert!(!value.to_string().contains("UNIQUE_PRIVATE_BODY_812"));
            }
            contract(name, &value, &ids);
        }
        std::fs::remove_file(dir.path().join("example.jsonl")).unwrap();
        state.catalog.as_ref().unwrap().test_due();
        state.catalog.as_ref().unwrap().test_parse();
        let (_, stale) = request(
            &app,
            "GET",
            &format!("/api/v1/sessions/{id}/workspace"),
            None,
            "",
        )
        .await;
        assert_eq!(stale["state"], "stale");
        contract("workspace-stale", &stale, &ids);
        let (code, recovering) = request(
            &app,
            "POST",
            &format!("/api/v1/sessions/{id}/retry"),
            Some("http://127.0.0.1:4317"),
            "",
        )
        .await;
        assert_eq!(code, StatusCode::ACCEPTED);
        assert_eq!(recovering["state"], "stale");
        assert!(recovering["error"].is_null());
        contract("workspace-recovering", &recovering, &ids);
        state.shutdown().await;
        let (failed_dir, failed_state, failed_app) = fixture();
        let failed_id = session(&failed_app).await;
        let (_, catalog) = request(&failed_app, "GET", "/api/v1/sessions", None, "").await;
        std::fs::remove_file(failed_dir.path().join("example.jsonl")).unwrap();
        request(
            &failed_app,
            "GET",
            &format!("/api/v1/sessions/{failed_id}/workspace"),
            None,
            "",
        )
        .await;
        failed_state.catalog.as_ref().unwrap().test_parse();
        let (_, failed) = request(
            &failed_app,
            "GET",
            &format!("/api/v1/sessions/{failed_id}/workspace"),
            None,
            "",
        )
        .await;
        let mut ids = std::collections::BTreeMap::new();
        ids.insert(failed_id, "synthetic-session".into());
        ids.insert(
            catalog["items"][0]["storeId"].as_str().unwrap().into(),
            "synthetic-store".into(),
        );
        ids.insert(
            failed["sourceRevision"].as_str().unwrap().into(),
            "synthetic-source-revision".into(),
        );
        assert_eq!(failed["state"], "failed");
        contract("workspace-failed", &failed, &ids);
        failed_state.shutdown().await;
    }
    #[tokio::test]
    async fn boundaries_reject_before_source_work() {
        let (_dir, state, app) = fixture();
        let id = session(&app).await;
        for endpoint in [
            "/api/v1/system".into(),
            "/api/v1/sessions".into(),
            format!("/api/v1/sessions/{id}/workspace"),
            format!("/api/v1/sessions/{id}/snapshots/forged/overview"),
        ] {
            assert_eq!(
                request(&app, "GET", &endpoint, Some("https://evil.test"), "")
                    .await
                    .0,
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                request(&app, "GET", &format!("{endpoint}?path=/private"), None, "")
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
            assert_eq!(
                request(&app, "GET", &endpoint, None, "private").await.0,
                StatusCode::BAD_REQUEST
            );
        }
        assert_eq!(
            request(
                &app,
                "POST",
                &format!("/api/v1/sessions/{id}/retry"),
                None,
                ""
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request(&app, "GET", "/api/v1/sessions/forged/workspace", None, "")
                .await
                .1["error"]["code"],
            "unknown_session"
        );
        assert_eq!(
            request(
                &app,
                "POST",
                "/api/v1/retry",
                Some("http://127.0.0.1:4317"),
                ""
            )
            .await
            .1["error"]["code"],
            "mode_conflict"
        );
        assert_eq!(
            state.catalog.as_ref().unwrap().status()["metrics"]["loadedEntries"],
            0
        );
        state.shutdown().await;
    }
}
