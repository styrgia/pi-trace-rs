use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, RawQuery, Request, State, rejection::PathRejection},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get, post},
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tower_http::services::ServeDir;
use trace_core::{
    DiagnosticQuery, ImportLimits, ObservationQuery, StructuralFilter, Trace,
    views::{ErrorResponse, StructuralError},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Workspace {
    Empty,
    Importing {
        #[serde(rename = "attemptId")]
        attempt_id: String,
    },
    Ready {
        #[serde(rename = "attemptId")]
        attempt_id: String,
        #[serde(rename = "snapshotId")]
        snapshot_id: String,
    },
    Failed {
        #[serde(rename = "attemptId")]
        attempt_id: String,
        error: StructuralError,
    },
}
enum LoadState {
    Empty,
    Importing {
        attempt_id: String,
    },
    Ready {
        attempt_id: String,
        trace: Arc<Trace>,
    },
    Failed {
        attempt_id: String,
        error: StructuralError,
    },
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveStatus {
    run_id: String,
    revision: Option<u64>,
    state: &'static str,
    activity: Option<trace_core::FollowProgress>,
    error: Option<StructuralError>,
}
struct ServiceState {
    load: LoadState,
    live: Option<LiveStatus>,
    run: Option<trace_core::RunId>,
    snapshots: std::collections::VecDeque<Arc<Trace>>,
    closing: bool,
    controls: Option<tokio::sync::mpsc::Sender<Control>>,
    owner: Option<tokio::task::JoinHandle<()>>,
}
enum Control {
    Pause(tokio::sync::oneshot::Sender<Result<(), StructuralError>>),
    Resume(tokio::sync::oneshot::Sender<Result<(), StructuralError>>),
    Shutdown(tokio::sync::oneshot::Sender<()>),
}
pub struct AppState {
    source: Option<PathBuf>,
    pub(crate) catalog: Option<Arc<crate::catalog::Catalog>>,
    limits: ImportLimits,
    state: Mutex<ServiceState>,
    sequence: AtomicU64,
    watch: bool,
    handlers: Arc<tokio::sync::Semaphore>,
}
impl AppState {
    #[cfg(test)]
    pub fn new(source: Option<PathBuf>) -> Arc<Self> {
        Self::with_mode(source, false)
    }
    pub fn with_mode(source: Option<PathBuf>, watch: bool) -> Arc<Self> {
        Arc::new(Self {
            source,
            catalog: None,
            watch,
            limits: ImportLimits::default(),
            state: Mutex::new(ServiceState {
                load: LoadState::Empty,
                live: None,
                run: None,
                snapshots: Default::default(),
                closing: false,
                controls: None,
                owner: None,
            }),
            sequence: AtomicU64::new(1),
            handlers: Arc::new(tokio::sync::Semaphore::new(8)),
        })
    }
    pub fn system(config: crate::system_config::SystemConfig) -> Arc<Self> {
        let mut state = Self::with_mode(None, false);
        Arc::get_mut(&mut state).unwrap().catalog = Some(crate::catalog::Catalog::new(config));
        state
    }
    pub fn begin(self: &Arc<Self>) {
        if let Some(catalog) = &self.catalog {
            catalog.begin();
            return;
        }
        let mut current = self.state.lock().unwrap();
        if !current.closing && self.source.is_some() && matches!(current.load, LoadState::Empty) {
            let attempt = self.reserve(&mut current);
            self.import_locked(&mut current, attempt);
        }
    }
    fn reserve(&self, current: &mut ServiceState) -> String {
        let attempt = format!("a{}", self.sequence.fetch_add(1, Ordering::Relaxed));
        current.load = LoadState::Importing {
            attempt_id: attempt.clone(),
        };
        if self.watch {
            let run = trace_core::RunId::new();
            current.run = Some(run.clone());
            current.live = Some(LiveStatus {
                run_id: run.as_str().into(),
                revision: None,
                state: "catchingUp",
                activity: None,
                error: None,
            });
        }
        attempt
    }
    #[cfg(test)]
    fn import(self: &Arc<Self>, attempt_id: String) {
        self.import_locked(&mut self.state.lock().unwrap(), attempt_id);
    }
    fn import_locked(self: &Arc<Self>, current: &mut ServiceState, attempt_id: String) {
        assert!(!current.closing);
        let source = self.source.clone().unwrap();
        let limits = self.limits;
        let state = self.clone();
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        current.controls = Some(tx);
        let run = current.run.clone().unwrap_or_default();
        let owner = tokio::spawn(async move {
            if !state.watch {
                let result = tokio::task::spawn_blocking(move || Trace::open(&source, limits))
                    .await
                    .unwrap_or_else(|_| {
                        Err(structural(
                            "import_worker_failed",
                            "The import worker could not complete.",
                        ))
                    });
                state.publish(result, &attempt_id);
                return;
            }
            let opened = tokio::task::spawn_blocking(move || {
                trace_core::FollowReader::open_in_run(&source, limits, run)
            })
            .await;
            let mut reader = match opened {
                Ok(Ok(reader)) => reader,
                Ok(Err(error)) => {
                    state.publish(Err(error), &attempt_id);
                    return;
                }
                Err(_) => {
                    state.publish(
                        Err(structural(
                            "import_worker_failed",
                            "The import worker could not complete.",
                        )),
                        &attempt_id,
                    );
                    return;
                }
            };
            let mut paused = false;
            let mut waiting = false;
            loop {
                let mut resume_ack = None;
                let control = if paused {
                    rx.recv().await
                } else if waiting {
                    tokio::select! { control = rx.recv() => control, _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => None }
                } else {
                    rx.try_recv().ok()
                };
                if let Some(control) = control {
                    match control {
                        Control::Shutdown(ack) => {
                            let _ = ack.send(());
                            break;
                        }
                        Control::Pause(ack) => {
                            paused = true;
                            state.follow_state("paused");
                            let _ = ack.send(Ok(()));
                            continue;
                        }
                        Control::Resume(ack) => {
                            if !paused {
                                let _ = ack.send(Ok(()));
                                continue;
                            }
                            paused = false;
                            state.follow_state("catchingUp");
                            resume_ack = Some(ack);
                        }
                    }
                }
                if paused {
                    continue;
                }
                let batch = tokio::task::spawn_blocking(move || {
                    let result = reader.update();
                    (reader, result)
                })
                .await;
                let result;
                (reader, result) = match batch {
                    Ok(batch) => batch,
                    Err(_) => {
                        state.publish(
                            Err(structural(
                                "import_worker_failed",
                                "The import worker could not complete.",
                            )),
                            &attempt_id,
                        );
                        break;
                    }
                };
                match result {
                    Ok(update) => {
                        waiting = !update.catching_up;
                        let mut current = state.state.lock().unwrap();
                        if let Some(trace) = update.snapshot {
                            Self::accept(&mut current, trace, &attempt_id);
                        }
                        let live = current.live.as_mut().unwrap();
                        live.activity = Some(update.progress);
                        live.state = if waiting { "waiting" } else { "catchingUp" };
                        if let Some(ack) = resume_ack {
                            let _ = ack.send(Ok(()));
                        }
                    }
                    Err(error) => {
                        state.publish(Err(error.clone()), &attempt_id);
                        if let Some(ack) = resume_ack {
                            let _ = ack.send(Err(error));
                        }
                        break;
                    }
                }
            }
        });
        current.owner = Some(owner);
    }
    fn accept(current: &mut ServiceState, trace: Trace, attempt_id: &str) {
        let trace = Arc::new(trace);
        if let Some(live) = current.live.as_mut() {
            if live.revision.is_none() {
                current.snapshots.clear();
            }
            live.revision = Some(live.revision.unwrap_or(0) + 1);
        }
        current.snapshots.push_back(trace.clone());
        while current.snapshots.len() > 2 {
            current.snapshots.pop_front();
        }
        current.load = LoadState::Ready {
            attempt_id: attempt_id.into(),
            trace,
        };
    }
    fn publish(&self, result: Result<Trace, StructuralError>, attempt_id: &str) {
        let mut current = self.state.lock().unwrap();
        match result {
            Ok(trace) => Self::accept(&mut current, trace, attempt_id),
            Err(error) => {
                let retain = current
                    .live
                    .as_ref()
                    .is_some_and(|live| live.revision.is_some());
                if let Some(live) = current.live.as_mut() {
                    live.state = "stopped";
                    live.error = Some(error.clone());
                }
                if retain {
                    current.load = LoadState::Ready {
                        attempt_id: attempt_id.into(),
                        trace: current.snapshots.back().unwrap().clone(),
                    };
                } else {
                    current.load = LoadState::Failed {
                        attempt_id: attempt_id.into(),
                        error,
                    };
                }
            }
        }
    }
    fn follow_state(&self, state: &'static str) {
        if let Some(live) = self.state.lock().unwrap().live.as_mut() {
            live.state = state;
        }
    }
    fn workspace_locked(current: &ServiceState) -> Workspace {
        match &current.load {
            LoadState::Empty => Workspace::Empty,
            LoadState::Importing { attempt_id } => Workspace::Importing {
                attempt_id: attempt_id.clone(),
            },
            LoadState::Ready { attempt_id, trace } => Workspace::Ready {
                attempt_id: attempt_id.clone(),
                snapshot_id: trace.snapshot_id().into(),
            },
            LoadState::Failed { attempt_id, error } => Workspace::Failed {
                attempt_id: attempt_id.clone(),
                error: error.clone(),
            },
        }
    }
    #[cfg(test)]
    fn workspace(&self) -> Workspace {
        Self::workspace_locked(&self.state.lock().unwrap())
    }
    fn workspace_json(&self) -> serde_json::Value {
        if self.catalog.is_some() {
            return serde_json::json!({"state":"system"});
        }
        let current = self.state.lock().unwrap();
        let mut value = serde_json::to_value(Self::workspace_locked(&current)).unwrap();
        if let Some(live) = &current.live {
            value["live"] = serde_json::to_value(live).unwrap();
        }
        value
    }
    fn snapshot(&self, id: &str) -> Result<Arc<Trace>, ApiError> {
        let current = self.state.lock().unwrap();
        if let Some(trace) = current
            .snapshots
            .iter()
            .find(|trace| trace.snapshot_id() == id)
        {
            return Ok(trace.clone());
        }
        if !current.snapshots.is_empty() {
            Err(api_error(
                StatusCode::NOT_FOUND,
                "unknown_snapshot",
                "The requested snapshot is unavailable.",
            ))
        } else {
            Err(api_error(
                StatusCode::CONFLICT,
                "workspace_unready",
                "The workspace has no completed snapshot.",
            ))
        }
    }
    fn retry(self: &Arc<Self>) -> Result<serde_json::Value, ApiError> {
        if self.catalog.is_some() {
            return Err(api_error(
                StatusCode::CONFLICT,
                "mode_conflict",
                "Use session-qualified recovery in system mode.",
            ));
        }
        let mut current = self.state.lock().unwrap();
        if current.closing {
            return Err(api_error(
                StatusCode::CONFLICT,
                "workspace_closing",
                "The workspace is shutting down.",
            ));
        }
        if self.source.is_none() {
            return Err(api_error(
                StatusCode::CONFLICT,
                "no_source",
                "No recording source was selected at startup.",
            ));
        }
        let stopped = current
            .live
            .as_ref()
            .is_some_and(|live| live.state == "stopped");
        if !(stopped || !self.watch && matches!(current.load, LoadState::Failed { .. })) {
            return Err(api_error(
                StatusCode::CONFLICT,
                "retry_conflict",
                "Only a failed import or stopped live run can be retried.",
            ));
        }
        let attempt_id = self.reserve(&mut current);
        let mut response = serde_json::to_value(Self::workspace_locked(&current)).unwrap();
        if let Some(live) = &current.live {
            response["live"] = serde_json::to_value(live).unwrap();
        }
        self.import_locked(&mut current, attempt_id);
        Ok(response)
    }
    async fn control(&self, pause: bool) -> Result<(), ApiError> {
        let tx = {
            let current = self.state.lock().unwrap();
            if current.closing {
                return Err(api_error(
                    StatusCode::CONFLICT,
                    "workspace_closing",
                    "The workspace is shutting down.",
                ));
            }
            let live = current.live.as_ref().ok_or_else(|| {
                api_error(
                    StatusCode::CONFLICT,
                    "mode_conflict",
                    "This workspace is not live.",
                )
            })?;
            if live.state == "stopped" {
                return Err(api_error(
                    StatusCode::CONFLICT,
                    "live_stopped",
                    "The live run has stopped.",
                ));
            }
            current.controls.clone().ok_or_else(|| {
                api_error(
                    StatusCode::CONFLICT,
                    "live_unavailable",
                    "The live worker is unavailable.",
                )
            })?
        };
        let (ack, done) = tokio::sync::oneshot::channel();
        tx.try_send(if pause {
            Control::Pause(ack)
        } else {
            Control::Resume(ack)
        })
        .map_err(|_| {
            api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "busy",
                "The live control queue is unavailable.",
            )
        })?;
        done.await
            .map_err(|_| {
                api_error(
                    StatusCode::CONFLICT,
                    "live_stopped",
                    "The live run has stopped.",
                )
            })?
            .map_err(|error| ApiError {
                status: StatusCode::CONFLICT,
                error,
            })
    }
    pub async fn shutdown(&self) {
        if let Some(catalog) = &self.catalog {
            catalog.shutdown().await;
            return;
        }
        let (tx, owner) = {
            let mut current = self.state.lock().unwrap();
            current.closing = true;
            (current.controls.take(), current.owner.take())
        };
        if let Some(tx) = tx {
            let (ack, done) = tokio::sync::oneshot::channel();
            if tx.send(Control::Shutdown(ack)).await.is_ok() {
                let _ = done.await;
            }
        }
        if let Some(owner) = owner {
            let _ = owner.await;
        }
    }
}

fn structural(code: &str, message: &str) -> StructuralError {
    StructuralError {
        code: code.into(),
        message: message.into(),
        source: None,
    }
}
#[derive(Debug)]
pub(crate) struct ApiError {
    status: StatusCode,
    error: StructuralError,
}
pub(crate) fn api_error(status: StatusCode, code: &str, message: &str) -> ApiError {
    ApiError {
        status,
        error: structural(code, message),
    }
}
impl From<StructuralError> for ApiError {
    fn from(error: StructuralError) -> Self {
        let status = match error.code.as_str() {
            "unknown_node" | "unknown_span" => StatusCode::NOT_FOUND,
            "view_limit" => StatusCode::PAYLOAD_TOO_LARGE,
            _ => StatusCode::BAD_REQUEST,
        };
        Self { status, error }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(ErrorResponse { error: self.error })).into_response()
    }
}
#[derive(Serialize)]
struct Health {
    status: &'static str,
    name: &'static str,
    version: &'static str,
    stage: &'static str,
}
async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        name: "pi-trace",
        version: env!("CARGO_PKG_VERSION"),
        stage: "bootstrap",
    })
}
async fn workspace(
    State(state): State<Arc<AppState>>,
    RawQuery(query): RawQuery,
) -> Result<Json<serde_json::Value>, ApiError> {
    no_query(query)?;
    Ok(Json(state.workspace_json()))
}
pub(crate) fn no_query(query: Option<String>) -> Result<(), ApiError> {
    if query.is_some() {
        Err(api_error(
            StatusCode::BAD_REQUEST,
            "invalid_query",
            "This endpoint accepts no query parameters.",
        ))
    } else {
        Ok(())
    }
}
async fn retry(
    State(state): State<Arc<AppState>>,
    RawQuery(query): RawQuery,
    request: Request,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    no_query(query)?;
    let bytes = to_bytes(request.into_body(), 0).await.map_err(|_| {
        api_error(
            StatusCode::BAD_REQUEST,
            "retry_payload",
            "Retry accepts no request body.",
        )
    })?;
    if !bytes.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "retry_payload",
            "Retry accepts no request body.",
        ));
    }
    Ok((StatusCode::ACCEPTED, Json(state.retry()?)))
}
async fn overview(
    State(state): State<Arc<AppState>>,
    path: Result<Path<String>, PathRejection>,
    RawQuery(query): RawQuery,
) -> Result<Json<trace_core::views::Overview>, ApiError> {
    let Path(snapshot) = path.map_err(|_| {
        api_error(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            "The API path is invalid.",
        )
    })?;
    no_query(query)?;
    Ok(Json(state.snapshot(&snapshot)?.overview()?))
}
pub(crate) fn parameters(
    raw: Option<String>,
    observations: bool,
) -> Result<(ObservationQuery, DiagnosticQuery), ApiError> {
    let mut query = ObservationQuery::default();
    let mut diagnostics = DiagnosticQuery::default();
    let mut seen = std::collections::HashSet::new();
    let invalid = || {
        api_error(
            StatusCode::BAD_REQUEST,
            "invalid_query",
            "The query contains an unsupported, repeated, or invalid parameter.",
        )
    };
    let raw = raw.unwrap_or_default();
    if raw.len() > 16384 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "query_limit",
            "The encoded query exceeds the request limit.",
        ));
    }
    for (key, value) in form_urlencoded::parse(raw.as_bytes()) {
        if !seen.insert(key.to_string()) {
            return Err(invalid());
        }
        match key.as_ref() {
            "q" if observations => query.q = value.into_owned(),
            "filter" if observations => {
                query.filter = Some(match value.as_ref() {
                    "error" => StructuralFilter::Error,
                    "tool" => StructuralFilter::Tool,
                    "generation" => StructuralFilter::Generation,
                    "slow" => StructuralFilter::Slow,
                    _ => return Err(invalid()),
                })
            }
            "cursor" => {
                query.cursor = Some(value.to_string());
                diagnostics.cursor = Some(value.into_owned());
            }
            "limit" => {
                let limit = value.parse::<usize>().map_err(|_| invalid())?;
                query.limit = limit;
                diagnostics.limit = limit;
            }
            _ => return Err(invalid()),
        }
    }
    Ok((query, diagnostics))
}
async fn observations(
    State(state): State<Arc<AppState>>,
    path: Result<Path<String>, PathRejection>,
    RawQuery(raw): RawQuery,
) -> Result<Json<trace_core::views::ObservationPage>, ApiError> {
    let Path(snapshot) = path.map_err(|_| {
        api_error(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            "The API path is invalid.",
        )
    })?;
    let (query, _) = parameters(raw, true)?;
    Ok(Json(state.snapshot(&snapshot)?.observations(&query)?))
}
async fn detail(
    State(state): State<Arc<AppState>>,
    path: Result<Path<(String, String)>, PathRejection>,
    RawQuery(query): RawQuery,
) -> Result<Json<trace_core::views::NodeDetail>, ApiError> {
    let Path((snapshot, node)) = path.map_err(|_| {
        api_error(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            "The API path is invalid.",
        )
    })?;
    no_query(query)?;
    Ok(Json(state.snapshot(&snapshot)?.node(&node)?))
}
async fn content(
    State(state): State<Arc<AppState>>,
    path: Result<Path<(String, String)>, PathRejection>,
    RawQuery(query): RawQuery,
) -> Result<Json<trace_core::views::NodeContent>, ApiError> {
    let Path((snapshot, node)) = path.map_err(|_| {
        api_error(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            "The API path is invalid.",
        )
    })?;
    no_query(query)?;
    Ok(Json(state.snapshot(&snapshot)?.content(&node)?))
}
async fn diagnostics(
    State(state): State<Arc<AppState>>,
    path: Result<Path<String>, PathRejection>,
    RawQuery(raw): RawQuery,
) -> Result<Json<trace_core::views::DiagnosticPage>, ApiError> {
    let Path(snapshot) = path.map_err(|_| {
        api_error(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            "The API path is invalid.",
        )
    })?;
    let (_, query) = parameters(raw, false)?;
    Ok(Json(state.snapshot(&snapshot)?.diagnostics(&query)?))
}
async fn unknown_api() -> ApiError {
    api_error(
        StatusCode::NOT_FOUND,
        "unknown_api_route",
        "The API route is unavailable.",
    )
}
async fn method_not_allowed() -> ApiError {
    api_error(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "The HTTP method is unsupported for this route.",
    )
}
async fn boundary(request: Request, next: Next) -> Response {
    let hosts: Vec<_> = request.headers().get_all(header::HOST).iter().collect();
    let origins: Vec<_> = request.headers().get_all(header::ORIGIN).iter().collect();
    let host_ok =
        hosts.len() == 1 && matches!(hosts[0].to_str(), Ok("127.0.0.1:4317" | "localhost:4317"));
    let origin_ok = origins.is_empty()
        || (origins.len() == 1
            && matches!(
                origins[0].to_str(),
                Ok("http://127.0.0.1:4317" | "http://localhost:4317" | "http://127.0.0.1:5173")
            ));
    let requires_origin = (request.uri().path().starts_with("/api/v1/sessions/")
        && request.uri().path().ends_with("/retry"))
        || matches!(
            request.uri().path(),
            "/api/v1/retry" | "/api/v1/live/pause" | "/api/v1/live/resume"
        );
    let mut response = if !host_ok {
        api_error(
            StatusCode::FORBIDDEN,
            "host_denied",
            "The Host header is not allowed.",
        )
        .into_response()
    } else if !origin_ok || (requires_origin && origins.is_empty()) {
        api_error(
            StatusCode::FORBIDDEN,
            "origin_denied",
            "The Origin header is not allowed for this request.",
        )
        .into_response()
    } else {
        next.run(request).await
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}
async fn live_control(
    State(state): State<Arc<AppState>>,
    RawQuery(query): RawQuery,
    request: Request,
) -> Result<Json<serde_json::Value>, ApiError> {
    no_query(query)?;
    let pause = request.uri().path().ends_with("/pause");
    to_bytes(request.into_body(), 0).await.map_err(|_| {
        api_error(
            StatusCode::BAD_REQUEST,
            "control_payload",
            "Live controls accept no request body.",
        )
    })?;
    state.control(pause).await?;
    Ok(Json(state.workspace_json()))
}
async fn span_detail(
    State(state): State<Arc<AppState>>,
    path: Result<Path<(String, String)>, PathRejection>,
    RawQuery(query): RawQuery,
) -> Result<Json<trace_core::views::NodeDetail>, ApiError> {
    let Path((snapshot, span)) = path.map_err(|_| {
        api_error(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            "The API path is invalid.",
        )
    })?;
    no_query(query)?;
    Ok(Json(state.snapshot(&snapshot)?.span(&span)?))
}
async fn view_bound(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let permit = if request.uri().path().starts_with("/api/v1/snapshots/")
        || request.uri().path().starts_with("/api/v1/sessions")
        || request.uri().path() == "/api/v1/system"
    {
        match state.handlers.clone().try_acquire_owned() {
            Ok(permit) => Some(permit),
            Err(_) => {
                return api_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "busy",
                    "The trace view handler limit is reached.",
                )
                .into_response();
            }
        }
    } else {
        None
    };
    let response = next.run(request).await;
    drop(permit);
    response
}
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(crate::system_routes::routes())
        .route("/api/v1/health", get(health))
        .route("/api/v1/workspace", get(workspace))
        .route("/api/v1/retry", post(retry))
        .route("/api/v1/live/pause", post(live_control))
        .route("/api/v1/live/resume", post(live_control))
        .route(
            "/api/v1/snapshots/{snapshot}/spans/{span}",
            get(span_detail),
        )
        .route("/api/v1/snapshots/{snapshot}/overview", get(overview))
        .route(
            "/api/v1/snapshots/{snapshot}/observations",
            get(observations),
        )
        .route("/api/v1/snapshots/{snapshot}/nodes/{node}", get(detail))
        .route(
            "/api/v1/snapshots/{snapshot}/nodes/{node}/content",
            get(content),
        )
        .route("/api/v1/snapshots/{snapshot}/diagnostics", get(diagnostics))
        .route("/api", any(unknown_api))
        .route("/api/{*rest}", any(unknown_api))
        .method_not_allowed_fallback(method_not_allowed)
        .fallback_service(ServeDir::new("web/dist"))
        .layer(middleware::from_fn_with_state(state.clone(), view_bound))
        .layer(middleware::from_fn(boundary))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use serde_json::{Value, json};
    use std::{fs, path::Path as FsPath};
    use tower::ServiceExt;

    async fn request(
        app: &Router,
        method: &str,
        path: &str,
        host: &str,
        origin: Option<&str>,
        body: &str,
    ) -> (StatusCode, axum::http::HeaderMap, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("host", host);
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::from(body.to_owned())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        (status, headers, serde_json::from_slice(&body).unwrap())
    }
    async fn get(app: &Router, path: &str) -> (StatusCode, axum::http::HeaderMap, Value) {
        request(app, "GET", path, "127.0.0.1:4317", None, "").await
    }
    async fn settled(state: &AppState) -> Workspace {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let state = state.workspace();
            if !matches!(state, Workspace::Importing { .. }) {
                return state;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "import worker did not finish"
            );
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    }
    async fn reference() -> (Arc<AppState>, Router, String) {
        let directory =
            FsPath::new(env!("CARGO_MANIFEST_DIR")).join("../trace-core/tests/fixtures/reference");
        let state = AppState::new(Some(directory));
        state.begin();
        let Workspace::Ready { snapshot_id, .. } = settled(&state).await else {
            panic!("reference did not import");
        };
        (state.clone(), router(state), snapshot_id)
    }
    #[tokio::test]
    async fn workspace_health_empty_importing_failed_and_ready_empty_are_distinct() {
        let state = AppState::new(None);
        let app = router(state);
        assert_eq!(
            get(&app, "/api/v1/workspace").await.2,
            json!({"state":"empty"})
        );
        let missing = tempfile::tempdir().unwrap();
        let state = AppState::new(Some(missing.path().to_path_buf()));
        state.begin();
        assert!(matches!(state.workspace(), Workspace::Importing { .. }));
        let app = router(state.clone());
        let health = get(&app, "/api/v1/health").await;
        assert_eq!(health.0, StatusCode::OK);
        assert_eq!(
            health.2,
            json!({"status":"ok","name":"pi-trace","version":env!("CARGO_PKG_VERSION"),"stage":"bootstrap"})
        );
        assert!(matches!(settled(&state).await, Workspace::Failed { .. }));
        assert_eq!(
            get(&app, "/api/v1/snapshots/anything/overview").await.0,
            StatusCode::CONFLICT
        );
        fs::write(missing.path().join("events.jsonl"), "").unwrap();
        let retry = request(
            &app,
            "POST",
            "/api/v1/retry",
            "localhost:4317",
            Some("http://localhost:4317"),
            "",
        )
        .await;
        assert_eq!(retry.0, StatusCode::ACCEPTED);
        let Workspace::Ready {
            snapshot_id,
            attempt_id,
        } = settled(&state).await
        else {
            panic!("repair retry did not publish");
        };
        assert_eq!(attempt_id, "a2");
        let overview = get(&app, &format!("/api/v1/snapshots/{snapshot_id}/overview")).await;
        assert_eq!(overview.2["nodeCount"], 0);
        assert_eq!(overview.2["snapshotId"], snapshot_id);
        assert_eq!(
            request(
                &app,
                "POST",
                "/api/v1/retry",
                "localhost:4317",
                Some("http://localhost:4317"),
                ""
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
    }
    #[tokio::test]
    async fn concurrent_retry_has_one_accepted_attempt_and_one_conflict() {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::new(Some(directory.path().to_path_buf()));
        state.begin();
        assert!(matches!(settled(&state).await, Workspace::Failed { .. }));
        let app = router(state.clone());
        let (first, second) = tokio::join!(
            request(
                &app,
                "POST",
                "/api/v1/retry",
                "localhost:4317",
                Some("http://localhost:4317"),
                ""
            ),
            request(
                &app,
                "POST",
                "/api/v1/retry",
                "localhost:4317",
                Some("http://localhost:4317"),
                ""
            )
        );
        assert_eq!(
            [first.0, second.0]
                .iter()
                .filter(|status| **status == StatusCode::ACCEPTED)
                .count(),
            1
        );
        assert_eq!(
            [first.0, second.0]
                .iter()
                .filter(|status| **status == StatusCode::CONFLICT)
                .count(),
            1
        );
        let Workspace::Failed { attempt_id, .. } = settled(&state).await else {
            panic!("source unexpectedly changed");
        };
        assert_eq!(attempt_id, "a2");
    }
    #[tokio::test]
    async fn local_boundaries_precede_retry_or_data_and_apply_no_store() {
        let (_, app, snapshot) = reference().await;
        let path = format!("/api/v1/snapshots/{snapshot}/overview");
        for (host, origin) in [
            ("evil.example", None),
            ("127.0.0.1:4317", Some("null")),
            ("localhost:4317", Some("https://evil.example")),
            ("localhost:4317", Some("http://localhost:5173")),
        ] {
            let response = request(&app, "GET", &path, host, origin, "").await;
            assert_eq!(response.0, StatusCode::FORBIDDEN);
            assert_eq!(response.1["cache-control"], "no-store");
            assert!(!response.1.contains_key("access-control-allow-origin"));
        }
        for (host, origin) in [
            ("localhost:4317", None),
            ("127.0.0.1:4317", Some("http://127.0.0.1:5173")),
            ("127.0.0.1:4317", Some("http://127.0.0.1:4317")),
        ] {
            let response = request(&app, "GET", &path, host, origin, "").await;
            assert_eq!(response.0, StatusCode::OK);
            assert_eq!(response.1["cache-control"], "no-store");
            assert!(!response.1.contains_key("access-control-allow-origin"));
        }
        assert_eq!(
            request(&app, "POST", "/api/v1/retry", "localhost:4317", None, "")
                .await
                .2["error"]["code"],
            "origin_denied"
        );
        let state = AppState::new(None);
        let app = router(state);
        assert_eq!(
            request(
                &app,
                "POST",
                "/api/v1/retry",
                "localhost:4317",
                Some("http://localhost:4317"),
                ""
            )
            .await
            .2["error"]["code"],
            "no_source"
        );
    }
    #[tokio::test]
    async fn payload_source_selection_queries_and_unknown_routes_return_structural_json() {
        let (_, app, snapshot) = reference().await;
        for path in [
            "/api/unknown".to_owned(),
            "/api".to_owned(),
            format!("/api/v1/snapshots/{snapshot}/nodes/missing"),
            "/api/v1/snapshots/stale/overview".to_owned(),
        ] {
            let response = get(&app, &path).await;
            assert_eq!(response.0, StatusCode::NOT_FOUND);
            assert!(response.2["error"]["code"].is_string());
            assert!(
                response.1["content-type"]
                    .to_str()
                    .unwrap()
                    .starts_with("application/json")
            );
        }
        assert_eq!(
            get(&app, &format!("/api/v1/snapshots/{snapshot}/nodes/missing"))
                .await
                .2["error"]["code"],
            "unknown_node"
        );
        assert_eq!(
            get(&app, "/api/v1/snapshots/stale/overview").await.2["error"]["code"],
            "unknown_snapshot"
        );
        for suffix in [
            "observations?source=/fake/private",
            "observations?filter=invalid",
            "observations?limit=201",
            "observations?limit=1&limit=2",
            "diagnostics?q=body",
            "overview?source=/fake/private",
            "nodes/n1?source=/fake/private",
            "nodes/n1/content?source=/fake/private",
        ] {
            assert_eq!(
                get(&app, &format!("/api/v1/snapshots/{snapshot}/{suffix}"))
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
        for (path, body) in [
            ("/api/v1/retry", "{\"source\":\"/fake/private\"}"),
            ("/api/v1/retry?source=/fake/private", ""),
        ] {
            let response = request(
                &app,
                "POST",
                path,
                "localhost:4317",
                Some("http://localhost:4317"),
                body,
            )
            .await;
            assert_eq!(response.0, StatusCode::BAD_REQUEST);
            assert!(!response.2.to_string().contains("/fake/private"));
        }
        assert_eq!(
            request(
                &app,
                "POST",
                "/api/v1/workspace",
                "localhost:4317",
                None,
                ""
            )
            .await
            .0,
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
    #[tokio::test]
    async fn metadata_routes_never_disclose_bodies_and_content_is_explicit() {
        let (_, app, snapshot) = reference().await;
        for endpoint in [
            "overview",
            "observations",
            "diagnostics",
            "nodes/n4",
            "nodes/n5",
        ] {
            let response = get(&app, &format!("/api/v1/snapshots/{snapshot}/{endpoint}")).await;
            assert_eq!(response.0, StatusCode::OK);
            assert_eq!(response.2["snapshotId"], snapshot);
            assert!(!response.2.to_string().contains("FABRICATED_PRIVATE"));
            assert!(!response.2.to_string().contains("invalid.example"));
        }
        let content = get(
            &app,
            &format!("/api/v1/snapshots/{snapshot}/nodes/n5/content"),
        )
        .await;
        assert!(
            content.2["input"]["text"]
                .as_str()
                .unwrap()
                .contains("FABRICATED_PRIVATE")
        );
        assert_eq!(content.1["cache-control"], "no-store");
        let page = get(
            &app,
            &format!("/api/v1/snapshots/{snapshot}/observations?filter=error&limit=2"),
        )
        .await;
        assert_eq!(page.2["totalMatches"], 1);
        assert!(page.2["nextCursor"].is_string());
    }
    async fn live_wait(state: &AppState, expected: &str, revision: Option<u64>) -> Value {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let value = state.workspace_json();
            if value["live"]["state"] == expected
                && revision.is_none_or(|revision| {
                    value["live"]["revision"]
                        .as_u64()
                        .is_some_and(|actual| actual >= revision)
                })
            {
                return value;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "live state did not settle: {value}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    fn append(path: &FsPath, text: &str) {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }
    async fn control_request(
        app: &Router,
        verb: &str,
    ) -> (StatusCode, axum::http::HeaderMap, Value) {
        request(
            app,
            "POST",
            &format!("/api/v1/live/{verb}"),
            "localhost:4317",
            Some("http://localhost:4317"),
            "",
        )
        .await
    }
    #[tokio::test]
    async fn live_waiting_revisions_retention_span_lookup_and_no_op_scans() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        fs::write(&path, "").unwrap();
        let state = AppState::with_mode(Some(directory.path().into()), true);
        assert_eq!(state.workspace_json(), json!({"state":"empty"}));
        state.begin();
        assert!(matches!(state.workspace(), Workspace::Importing { .. }));
        let app = router(state.clone());
        let first = live_wait(&state, "waiting", Some(1)).await;
        let snapshot1 = first["snapshotId"].as_str().unwrap().to_owned();
        assert_eq!(
            state.snapshot(&snapshot1).unwrap().run_id(),
            first["live"]["runId"]
        );
        tokio::time::sleep(std::time::Duration::from_millis(550)).await;
        assert_eq!(state.workspace_json(), first);
        append(
            &path,
            "{\"type\":\"session_start\",\"ts\":0,\"sessionId\":\"synthetic\"}\n",
        );
        let second = live_wait(&state, "waiting", Some(2)).await;
        let snapshot2 = second["snapshotId"].as_str().unwrap().to_owned();
        let page = get(&app, &format!("/api/v1/snapshots/{snapshot2}/observations"))
            .await
            .2;
        let span = page["items"][0]["spanId"].as_str().unwrap();
        let detail = get(&app, &format!("/api/v1/snapshots/{snapshot2}/spans/{span}")).await;
        assert_eq!(detail.0, StatusCode::OK);
        assert_eq!(detail.2["observation"]["spanId"], span);
        assert_eq!(
            get(&app, &format!("/api/v1/snapshots/{snapshot1}/overview"))
                .await
                .0,
            StatusCode::OK
        );
        append(
            &path,
            "{\"type\":\"interaction_start\",\"ts\":1,\"sessionId\":\"synthetic\",\"interactionId\":1,\"prompt\":\"FABRICATED_PRIVATE_LIVE\"}\n",
        );
        let third = live_wait(&state, "waiting", Some(3)).await;
        let snapshot3 = third["snapshotId"].as_str().unwrap();
        assert_eq!(
            get(&app, &format!("/api/v1/snapshots/{snapshot1}/overview"))
                .await
                .2["error"]["code"],
            "unknown_snapshot"
        );
        assert_eq!(
            get(&app, &format!("/api/v1/snapshots/{snapshot2}/overview"))
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            get(&app, &format!("/api/v1/snapshots/{snapshot3}/spans/{span}"))
                .await
                .2["observation"]["spanId"],
            span
        );
        let filtered = get(
            &app,
            &format!("/api/v1/snapshots/{snapshot3}/observations?filter=tool"),
        )
        .await
        .2;
        assert_eq!(filtered["totalMatches"], 0);
        assert!(!third.to_string().contains("FABRICATED_PRIVATE"));
        assert!(!detail.2.to_string().contains("FABRICATED_PRIVATE"));
        let paged = get(
            &app,
            &format!("/api/v1/snapshots/{snapshot3}/observations?limit=1"),
        )
        .await
        .2;
        let cursor = paged["nextCursor"].as_str().unwrap();
        let cross = get(
            &app,
            &format!("/api/v1/snapshots/{snapshot2}/observations?limit=1&cursor={cursor}"),
        )
        .await;
        assert_eq!(cross.0, StatusCode::BAD_REQUEST);
        assert_eq!(cross.2["error"]["code"], "invalid_cursor");
        assert_eq!(
            get(
                &app,
                &format!("/api/v1/snapshots/{snapshot3}/spans/{span}?source=/fabricated/private")
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        let permits = state.handlers.clone().acquire_many_owned(8).await.unwrap();
        let busy = get(&app, &format!("/api/v1/snapshots/{snapshot3}/overview")).await;
        assert_eq!(busy.0, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(busy.2["error"]["code"], "busy");
        assert_eq!(busy.1["cache-control"], "no-store");
        drop(permits);
        state.shutdown().await;
    }
    #[tokio::test]
    async fn shared_pause_backlog_resume_failure_and_atomic_reset() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        fs::write(&path, "").unwrap();
        let state = AppState::with_mode(Some(directory.path().into()), true);
        state.begin();
        live_wait(&state, "waiting", Some(1)).await;
        let first_client = router(state.clone());
        let second_client = router(state.clone());
        let paused = control_request(&first_client, "pause").await;
        assert_eq!(paused.0, StatusCode::OK);
        assert_eq!(paused.2["live"]["state"], "paused");
        assert_eq!(control_request(&second_client, "pause").await.2, paused.2);
        let backlog = "{\"type\":\"synthetic_unknown\"}\n".repeat(4500);
        append(&path, &backlog);
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert_eq!(get(&second_client, "/api/v1/workspace").await.2, paused.2);
        let resume = control_request(&second_client, "resume").await;
        assert_eq!(resume.0, StatusCode::OK);
        let caught_up = live_wait(&state, "waiting", Some(2)).await;
        assert_eq!(caught_up["live"]["activity"]["recordCount"], 4500);
        assert_eq!(control_request(&first_client, "resume").await.2, caught_up);
        control_request(&first_client, "pause").await;
        fs::write(&path, "").unwrap();
        let rejected = control_request(&second_client, "resume").await;
        assert_eq!(rejected.0, StatusCode::CONFLICT);
        let stopped = live_wait(&state, "stopped", None).await;
        assert_eq!(stopped["state"], "ready");
        assert_eq!(stopped["snapshotId"], caught_up["snapshotId"]);
        assert_eq!(stopped["live"]["revision"], caught_up["live"]["revision"]);
        assert_eq!(get(&first_client, "/api/v1/health").await.0, StatusCode::OK);
        let retry = || {
            request(
                &first_client,
                "POST",
                "/api/v1/retry",
                "localhost:4317",
                Some("http://localhost:4317"),
                "",
            )
        };
        let (a, b) = tokio::join!(retry(), retry());
        assert_eq!(
            [a.0, b.0]
                .iter()
                .filter(|status| **status == StatusCode::ACCEPTED)
                .count(),
            1
        );
        let accepted = if a.0 == StatusCode::ACCEPTED {
            a.2
        } else {
            b.2
        };
        assert_eq!(accepted["live"]["revision"], Value::Null);
        assert_ne!(accepted["live"]["runId"], stopped["live"]["runId"]);
        let reset = live_wait(&state, "waiting", Some(1)).await;
        assert_eq!(reset["live"]["revision"], 1);
        assert_eq!(reset["live"]["runId"], accepted["live"]["runId"]);
        assert_eq!(
            get(
                &first_client,
                &format!(
                    "/api/v1/snapshots/{}/overview",
                    stopped["snapshotId"].as_str().unwrap()
                )
            )
            .await
            .2["error"]["code"],
            "unknown_snapshot"
        );
        state.shutdown().await;
        assert!(state.state.lock().unwrap().owner.is_none());
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
    }
    #[tokio::test]
    async fn live_controls_enforce_origin_payload_query_and_mode_boundaries() {
        let state = AppState::new(None);
        let app = router(state);
        for verb in ["pause", "resume"] {
            let route = format!("/api/v1/live/{verb}");
            for origin in [None, Some("null"), Some("https://invalid.example")] {
                let response = request(
                    &app,
                    "POST",
                    &route,
                    "localhost:4317",
                    origin,
                    "FABRICATED_PRIVATE_BODY",
                )
                .await;
                assert_eq!(response.0, StatusCode::FORBIDDEN);
                assert_eq!(response.1["cache-control"], "no-store");
                assert!(!response.2.to_string().contains("FABRICATED_PRIVATE"));
            }
            assert_eq!(
                control_request(&app, verb).await.2["error"]["code"],
                "mode_conflict"
            );
            for (suffix, body) in [
                ("?source=/fabricated/private", ""),
                ("", "FABRICATED_PRIVATE_BODY"),
            ] {
                let response = request(
                    &app,
                    "POST",
                    &format!("{route}{suffix}"),
                    "localhost:4317",
                    Some("http://localhost:4317"),
                    body,
                )
                .await;
                assert_eq!(response.0, StatusCode::BAD_REQUEST);
                assert!(!response.2.to_string().contains("FABRICATED_PRIVATE"));
                assert!(!response.2.to_string().contains("/fabricated/private"));
            }
        }
    }
    #[tokio::test]
    async fn paused_cumulative_record_and_pending_byte_budgets_do_not_reset() {
        for bytes in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("events.jsonl");
            fs::write(&path, "").unwrap();
            let mut state = AppState::with_mode(Some(directory.path().into()), true);
            if bytes {
                Arc::get_mut(&mut state).unwrap().limits.total_bytes = 4;
            } else {
                Arc::get_mut(&mut state).unwrap().limits.records = 2;
            }
            state.begin();
            live_wait(&state, "waiting", Some(1)).await;
            append(&path, if bytes { "{{{{" } else { "x\n{}\n" });
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                let value = state.workspace_json();
                let count = if bytes {
                    &value["live"]["activity"]["readBytes"]
                } else {
                    &value["live"]["activity"]["recordCount"]
                };
                if *count == if bytes { 4 } else { 2 } {
                    break;
                }
                assert!(std::time::Instant::now() < deadline);
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(550)).await;
            let app = router(state.clone());
            control_request(&app, "pause").await;
            let accepted = state.workspace_json();
            append(&path, if bytes { "{" } else { "{}\n" });
            assert_eq!(
                control_request(&app, "resume").await.0,
                StatusCode::CONFLICT
            );
            let stopped = live_wait(&state, "stopped", None).await;
            assert_eq!(stopped["snapshotId"], accepted["snapshotId"]);
            assert_eq!(stopped["live"]["revision"], accepted["live"]["revision"]);
            assert_eq!(
                stopped["live"]["error"]["code"],
                if bytes {
                    "input_byte_limit"
                } else {
                    "record_count_limit"
                }
            );
            state.shutdown().await;
        }
    }
    #[tokio::test]
    async fn shutdown_catchup_awaits_owner_and_never_writes_sources() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        let input = "{\"type\":\"synthetic_unknown\"}\n".repeat(12000);
        fs::write(&path, &input).unwrap();
        let state = AppState::with_mode(Some(directory.path().into()), true);
        state.begin();
        tokio::time::timeout(std::time::Duration::from_secs(5), state.shutdown())
            .await
            .unwrap();
        assert!(state.state.lock().unwrap().owner.is_none());
        assert!(state.state.lock().unwrap().controls.is_none());
        let stopped = state.workspace_json();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert_eq!(state.workspace_json(), stopped);
        assert_eq!(fs::read_to_string(path).unwrap(), input);
    }
    #[tokio::test]
    async fn live_initial_failure_stays_failed_and_never_changes_health() {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::with_mode(Some(directory.path().into()), true);
        state.begin();
        let stopped = live_wait(&state, "stopped", None).await;
        assert_eq!(stopped["state"], "failed");
        assert_eq!(stopped["live"]["revision"], Value::Null);
        let app = router(state.clone());
        assert_eq!(get(&app, "/api/v1/health").await.2["status"], "ok");
        state.shutdown().await;
    }
    fn live_example(name: &str, mut value: Value) {
        if value["live"].is_object() {
            value["live"]["runId"] = json!("synthetic-run");
        }
        if value["snapshotId"].is_string() {
            value["snapshotId"] = json!("synthetic-snapshot");
        }
        let path =
            FsPath::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/contracts/{name}.json"));
        let serialized = format!("{}\n", serde_json::to_string_pretty(&value).unwrap());
        if std::env::var_os("UPDATE_CONTRACTS").is_some() {
            fs::write(&path, &serialized).unwrap();
        }
        assert_eq!(fs::read_to_string(path).unwrap(), serialized);
    }
    #[tokio::test]
    async fn live_workspace_and_control_http_serialization_examples() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        fs::write(&path, "").unwrap();
        let state = AppState::with_mode(Some(directory.path().into()), true);
        {
            let mut current = state.state.lock().unwrap();
            state.reserve(&mut current);
        }
        let app = router(state.clone());
        live_example(
            "workspace-live-importing",
            get(&app, "/api/v1/workspace").await.2,
        );
        state.import("a1".into());
        live_wait(&state, "waiting", Some(1)).await;
        live_example(
            "workspace-live-ready",
            get(&app, "/api/v1/workspace").await.2,
        );
        live_example(
            "control-live-paused",
            control_request(&app, "pause").await.2,
        );
        live_example(
            "control-live-resumed",
            control_request(&app, "resume").await.2,
        );
        control_request(&app, "pause").await;
        fs::remove_file(path).unwrap();
        control_request(&app, "resume").await;
        live_wait(&state, "stopped", None).await;
        live_example(
            "workspace-live-stopped",
            get(&app, "/api/v1/workspace").await.2,
        );
        state.shutdown().await;
    }
    #[tokio::test]
    async fn source_and_traversal_limits_survive_idle_and_paused_backlogs() {
        for recognized in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            fs::write(directory.path().join("events.jsonl"), "").unwrap();
            fs::create_dir_all(directory.path().join("subagents")).unwrap();
            if recognized {
                fs::create_dir_all(directory.path().join("subagents/a")).unwrap();
                fs::write(directory.path().join("subagents/a/events.jsonl"), "").unwrap();
            } else {
                fs::write(directory.path().join("subagents/ignored-a"), "").unwrap();
            }
            let mut state = AppState::with_mode(Some(directory.path().into()), true);
            Arc::get_mut(&mut state).unwrap().limits.sources = 2;
            state.begin();
            let accepted = live_wait(&state, "waiting", Some(1)).await;
            tokio::time::sleep(std::time::Duration::from_millis(550)).await;
            let app = router(state.clone());
            assert_eq!(control_request(&app, "pause").await.0, StatusCode::OK);
            if recognized {
                fs::create_dir_all(directory.path().join("subagents/b")).unwrap();
                fs::write(directory.path().join("subagents/b/events.jsonl"), "").unwrap();
            } else {
                fs::remove_file(directory.path().join("subagents/ignored-a")).unwrap();
                fs::write(directory.path().join("subagents/ignored-b"), "").unwrap();
            }
            assert_eq!(
                control_request(&app, "resume").await.0,
                StatusCode::CONFLICT
            );
            let stopped = live_wait(&state, "stopped", None).await;
            assert_eq!(stopped["live"]["error"]["code"], "source_entry_limit");
            assert_eq!(stopped["snapshotId"], accepted["snapshotId"]);
            assert_eq!(stopped["live"]["revision"], accepted["live"]["revision"]);
            state.shutdown().await;
        }
    }
    #[tokio::test]
    async fn shutdown_reserves_closing_before_wait_and_rejects_stopped_retry() {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::with_mode(Some(directory.path().into()), true);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let (entered, boundary) = tokio::sync::oneshot::channel();
        let (release, finish) = tokio::sync::oneshot::channel();
        {
            let mut current = state.state.lock().unwrap();
            state.reserve(&mut current);
            current.load = LoadState::Failed {
                attempt_id: "a1".into(),
                error: structural("synthetic_failure", "Synthetic failure."),
            };
            current.live.as_mut().unwrap().state = "stopped";
            current.controls = Some(tx);
            current.owner = Some(tokio::spawn(async move {
                let Some(Control::Shutdown(ack)) = rx.recv().await else {
                    panic!("shutdown control missing");
                };
                entered.send(()).unwrap();
                finish.await.unwrap();
                let _ = ack.send(());
            }));
        }
        let closing_state = state.clone();
        let closing = tokio::spawn(async move {
            closing_state.shutdown().await;
        });
        boundary.await.unwrap();
        assert!(!closing.is_finished());
        let rejected = state.retry().unwrap_err();
        assert_eq!(rejected.status, StatusCode::CONFLICT);
        assert_eq!(rejected.error.code, "workspace_closing");
        state.begin();
        {
            let current = state.state.lock().unwrap();
            assert!(current.closing);
            assert!(current.controls.is_none());
            assert!(current.owner.is_none());
        }
        release.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), closing)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(state.sequence.load(Ordering::Relaxed), 2);
        assert_eq!(state.retry().unwrap_err().error.code, "workspace_closing");
    }
    #[tokio::test]
    async fn resume_failure_is_committed_before_ack_and_failed_reset_has_no_current_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("events.jsonl");
        fs::write(
            &path,
            "{\"type\":\"session_start\",\"sessionId\":\"synthetic\"}\n",
        )
        .unwrap();
        let state = AppState::with_mode(Some(directory.path().into()), true);
        state.begin();
        let accepted = live_wait(&state, "waiting", Some(1)).await;
        state.control(true).await.unwrap();
        fs::remove_file(&path).unwrap();
        assert!(state.control(false).await.is_err());
        let stopped = state.workspace_json();
        assert_eq!(stopped["live"]["state"], "stopped");
        assert!(stopped["live"]["error"]["code"].is_string());
        assert_eq!(stopped["snapshotId"], accepted["snapshotId"]);
        let replacement = state.retry().unwrap();
        assert_ne!(replacement["live"]["runId"], accepted["live"]["runId"]);
        let failed = live_wait(&state, "stopped", None).await;
        assert_eq!(failed["state"], "failed");
        assert_eq!(failed["live"]["runId"], replacement["live"]["runId"]);
        assert_eq!(failed["live"]["revision"], Value::Null);
        assert!(failed.get("snapshotId").is_none());
        let old = accepted["snapshotId"].as_str().unwrap();
        assert_eq!(
            state.snapshot(old).unwrap().run_id(),
            accepted["live"]["runId"]
        );
        let app = router(state.clone());
        assert_eq!(
            get(&app, &format!("/api/v1/snapshots/{old}/overview"))
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(get(&app, "/api/v1/workspace").await.2, failed);
        state.shutdown().await;
    }
    #[test]
    fn workspace_contract_examples() {
        let values = [
            ("workspace-empty", Workspace::Empty),
            (
                "workspace-importing",
                Workspace::Importing {
                    attempt_id: "a1".into(),
                },
            ),
            (
                "workspace-ready",
                Workspace::Ready {
                    attempt_id: "a1".into(),
                    snapshot_id: "synthetic-snapshot".into(),
                },
            ),
            (
                "workspace-failed",
                Workspace::Failed {
                    attempt_id: "a1".into(),
                    error: StructuralError {
                        code: "source_unreadable".into(),
                        message: "The recording source could not be read.".into(),
                        source: Some(trace_core::views::SourceRef {
                            file: "events.jsonl".into(),
                            line: 1,
                        }),
                    },
                },
            ),
        ];
        for (name, value) in values {
            let path = FsPath::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("tests/contracts/{name}.json"));
            let serialized = format!("{}\n", serde_json::to_string_pretty(&value).unwrap());
            if std::env::var_os("UPDATE_CONTRACTS").is_some() {
                fs::write(&path, &serialized).unwrap();
            }
            assert_eq!(fs::read_to_string(path).unwrap(), serialized);
        }
    }
    #[tokio::test]
    async fn system_routes_share_the_existing_eight_handler_boundary() {
        let state = AppState::system(crate::system_config::SystemConfig {
            stores: vec![],
            disabled: vec![],
        });
        let app = router(state.clone());
        let permits = state.handlers.clone().acquire_many_owned(8).await.unwrap();
        for path in [
            "/api/v1/system",
            "/api/v1/sessions",
            "/api/v1/sessions/forged/workspace",
            "/api/v1/sessions/forged/snapshots/forged/overview",
        ] {
            let (status, _, body) = get(&app, path).await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(body["error"]["code"], "busy");
        }
        drop(permits);
        assert_eq!(get(&app, "/api/v1/system").await.0, StatusCode::OK);
    }
}
