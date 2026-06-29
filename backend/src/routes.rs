use std::sync::{Arc, Mutex};

use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{Days, NaiveDate};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::db;
use crate::models::{
    AppSettings, BootstrapData, PersistedScheduleRun, ScheduleRunStatus, SessionStatus,
};
use crate::priority::EloUpdate;
use crate::scheduler::{ScheduleInput, SchedulePreview, plan_schedule};

#[derive(Clone)]
pub struct AppState {
    conn: Arc<Mutex<Connection>>,
}

pub fn router(conn: Connection) -> Router {
    let state = AppState {
        conn: Arc::new(Mutex::new(conn)),
    };

    Router::new()
        .route("/api/health", get(health))
        .route("/api/bootstrap", get(get_bootstrap).put(put_bootstrap))
        .route("/api/settings", post(put_settings).put(put_settings))
        .route("/api/priority/comparisons", post(post_priority_comparison))
        .route("/api/schedules/generate", post(post_generate_schedule))
        .route("/api/schedules/current", get(get_current_schedule))
        .route("/api/schedules/previous", get(get_previous_schedule))
        .route("/api/schedules/references", get(get_reference_schedules))
        .route("/api/schedules/{id}/pin", post(post_pin_schedule))
        .route("/api/planner/simulate", post(post_simulate_plan))
        .route(
            "/api/sessions/{id}",
            post(patch_session).patch(patch_session),
        )
        .route("/api/sessions/{id}/postpone", post(postpone_session))
        .route("/api/sessions/{id}/log", post(post_log_session))
        .route("/api/logs", post(post_manual_log))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn get_bootstrap(State(state): State<AppState>) -> ApiResult<Json<AppSnapshot>> {
    let conn = state.lock()?;
    Ok(Json(snapshot(&conn)?))
}

async fn put_bootstrap(
    State(state): State<AppState>,
    Json(payload): Json<BootstrapData>,
) -> ApiResult<Json<AppSnapshot>> {
    let conn = state.lock()?;
    db::save_bootstrap(&conn, &payload)?;
    Ok(Json(snapshot(&conn)?))
}

async fn put_settings(
    State(state): State<AppState>,
    Json(settings): Json<AppSettings>,
) -> ApiResult<Json<AppSnapshot>> {
    let conn = state.lock()?;
    db::save_settings(&conn, &settings)?;
    Ok(Json(snapshot(&conn)?))
}

async fn post_priority_comparison(
    State(state): State<AppState>,
    Json(payload): Json<PriorityComparisonRequest>,
) -> ApiResult<Json<PriorityComparisonResponse>> {
    let conn = state.lock()?;
    let update = db::apply_priority_comparison(
        &conn,
        &payload.winner_topic_id,
        &payload.loser_topic_id,
        payload.k_factor.unwrap_or(32.0),
    )?;
    Ok(Json(PriorityComparisonResponse {
        update,
        topics: db::list_topics(&conn)?,
    }))
}

async fn post_generate_schedule(
    State(state): State<AppState>,
    Json(payload): Json<GenerateScheduleRequest>,
) -> ApiResult<Json<GenerateScheduleResponse>> {
    let conn = state.lock()?;
    let bootstrap = db::load_bootstrap(&conn)?;
    let start_date = payload
        .start_date
        .unwrap_or_else(|| chrono::Local::now().date_naive());
    let end_date = payload
        .end_date
        .unwrap_or_else(|| default_end_date(start_date, &bootstrap));
    let preview = plan_schedule(ScheduleInput {
        start_date,
        end_date,
        settings: bootstrap.settings,
        topics: bootstrap.topics,
        study_windows: bootstrap.study_windows,
        blocked_intervals: bootstrap.blocked_intervals,
        capacity_overrides: bootstrap.capacity_overrides,
        last_studied_dates: db::last_studied_dates(&conn)?,
    });

    let saved = if preview.can_generate && payload.persist.unwrap_or(true) {
        Some(db::save_current_schedule(&conn, &preview)?)
    } else {
        None
    };

    Ok(Json(GenerateScheduleResponse { preview, saved }))
}

async fn get_current_schedule(
    State(state): State<AppState>,
) -> ApiResult<Json<Option<PersistedScheduleRun>>> {
    let conn = state.lock()?;
    Ok(Json(db::get_schedule_by_status(
        &conn,
        ScheduleRunStatus::Current,
    )?))
}

async fn get_previous_schedule(
    State(state): State<AppState>,
) -> ApiResult<Json<Option<PersistedScheduleRun>>> {
    let conn = state.lock()?;
    Ok(Json(db::get_schedule_by_status(
        &conn,
        ScheduleRunStatus::Previous,
    )?))
}

async fn get_reference_schedules(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<PersistedScheduleRun>>> {
    let conn = state.lock()?;
    Ok(Json(db::list_reference_schedules(&conn)?))
}

async fn post_pin_schedule(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<PinScheduleRequest>,
) -> ApiResult<Json<AppSnapshot>> {
    let conn = state.lock()?;
    db::pin_schedule_reference(&conn, &id, payload.name.as_deref())?;
    Ok(Json(snapshot(&conn)?))
}

async fn post_simulate_plan(
    Json(payload): Json<ScheduleInput>,
) -> ApiResult<Json<SchedulePreview>> {
    Ok(Json(plan_schedule(payload)))
}

async fn patch_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<SessionUpdateRequest>,
) -> ApiResult<Json<crate::models::PersistedSession>> {
    let conn = state.lock()?;
    Ok(Json(db::update_session(
        &conn,
        &id,
        payload.date,
        payload.start_minute,
        payload.end_minute,
        payload.locked,
        payload.status,
    )?))
}

async fn postpone_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<PostponeSessionRequest>,
) -> ApiResult<Json<crate::models::PersistedSession>> {
    let conn = state.lock()?;
    Ok(Json(db::update_session(
        &conn,
        &id,
        payload.date,
        payload.start_minute,
        payload.end_minute,
        payload.locked.unwrap_or(true),
        SessionStatus::Planned,
    )?))
}

async fn post_log_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<StudyLogRequest>,
) -> ApiResult<Json<AppSnapshot>> {
    let conn = state.lock()?;
    db::log_session(
        &conn,
        Some(&id),
        &payload.topic_id,
        payload.date,
        payload.minutes,
        payload.note.as_deref().unwrap_or(""),
        payload.status,
    )?;
    Ok(Json(snapshot(&conn)?))
}

async fn post_manual_log(
    State(state): State<AppState>,
    Json(payload): Json<StudyLogRequest>,
) -> ApiResult<Json<AppSnapshot>> {
    let conn = state.lock()?;
    db::log_session(
        &conn,
        None,
        &payload.topic_id,
        payload.date,
        payload.minutes,
        payload.note.as_deref().unwrap_or(""),
        payload.status,
    )?;
    Ok(Json(snapshot(&conn)?))
}

fn snapshot(conn: &Connection) -> Result<AppSnapshot> {
    Ok(AppSnapshot {
        bootstrap: db::load_bootstrap(conn)?,
        current_schedule: db::get_schedule_by_status(conn, ScheduleRunStatus::Current)?,
        previous_schedule: db::get_schedule_by_status(conn, ScheduleRunStatus::Previous)?,
        reference_schedules: db::list_reference_schedules(conn)?,
    })
}

fn default_end_date(start_date: NaiveDate, bootstrap: &BootstrapData) -> NaiveDate {
    bootstrap
        .topics
        .iter()
        .filter(|topic| !topic.archived && topic.target_minutes > topic.completed_minutes)
        .filter_map(|topic| topic.deadline)
        .max()
        .unwrap_or_else(|| {
            start_date
                .checked_add_days(Days::new(90))
                .expect("90 day fallback horizon should be representable")
        })
}

impl AppState {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock was poisoned"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub bootstrap: BootstrapData,
    pub current_schedule: Option<PersistedScheduleRun>,
    pub previous_schedule: Option<PersistedScheduleRun>,
    pub reference_schedules: Vec<PersistedScheduleRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriorityComparisonRequest {
    pub winner_topic_id: String,
    pub loser_topic_id: String,
    pub k_factor: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriorityComparisonResponse {
    pub update: EloUpdate,
    pub topics: Vec<crate::models::Topic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateScheduleRequest {
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub persist: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateScheduleResponse {
    pub preview: SchedulePreview,
    pub saved: Option<PersistedScheduleRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinScheduleRequest {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdateRequest {
    pub date: NaiveDate,
    pub start_minute: i64,
    pub end_minute: i64,
    pub locked: bool,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostponeSessionRequest {
    pub date: NaiveDate,
    pub start_minute: i64,
    pub end_minute: i64,
    pub locked: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyLogRequest {
    pub topic_id: String,
    pub date: NaiveDate,
    pub minutes: i64,
    pub note: Option<String>,
    pub status: Option<SessionStatus>,
}

type ApiResult<T> = std::result::Result<T, ApiError>;

#[derive(Debug)]
struct ApiError(anyhow::Error);

impl<E> From<E> for ApiError
where
    E: Into<anyhow::Error>,
{
    fn from(value: E) -> Self {
        Self(value.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::INTERNAL_SERVER_ERROR;
        let body = Json(json!({ "error": self.0.to_string() }));
        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use uuid::Uuid;

    use crate::models::{AvailabilityWindow, CapacityOverride, Topic, WindowKind};

    #[tokio::test]
    async fn bootstrap_returns_seeded_topics() {
        let app = router(db::open_memory_database().expect("database"));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/bootstrap")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let snapshot: AppSnapshot = read_json(response).await;
        assert_eq!(snapshot.bootstrap.topics.len(), 16);
    }

    #[tokio::test]
    async fn priority_comparison_updates_elo() {
        let app = router(db::open_memory_database().expect("database"));
        let snapshot = get_snapshot(app.clone()).await;
        let winner = snapshot.bootstrap.topics[0].id.clone();
        let loser = snapshot.bootstrap.topics[1].id.clone();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/priority/comparisons")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&PriorityComparisonRequest {
                            winner_topic_id: winner.clone(),
                            loser_topic_id: loser.clone(),
                            k_factor: Some(32.0),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body: PriorityComparisonResponse = read_json(response).await;
        let winner_after = body.topics.iter().find(|topic| topic.id == winner).unwrap();
        let loser_after = body.topics.iter().find(|topic| topic.id == loser).unwrap();
        assert!(winner_after.elo > 1050.0);
        assert!(loser_after.elo < 1050.0);
    }

    #[tokio::test]
    async fn generation_with_missing_deadlines_does_not_persist() {
        let app = router(db::open_memory_database().expect("database"));
        let snapshot = get_snapshot(app.clone()).await;
        let mut bootstrap = snapshot.bootstrap;
        bootstrap.topics[0].target_minutes = 120;
        save_bootstrap(app.clone(), &bootstrap).await;

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/schedules/generate")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body: GenerateScheduleResponse = read_json(response).await;
        assert!(!body.preview.can_generate);
        assert!(body.saved.is_none());
    }

    #[tokio::test]
    async fn generation_persists_current_and_retains_previous() {
        let app = router(db::open_memory_database().expect("database"));
        let mut snapshot = get_snapshot(app.clone()).await;
        let start = NaiveDate::from_ymd_opt(2026, 6, 29).unwrap();
        snapshot.bootstrap.settings.default_daily_topic_cap = 1;
        snapshot.bootstrap.topics = vec![test_topic("linear", "Linear Algebra", 45, start)];
        snapshot.bootstrap.study_windows = vec![AvailabilityWindow {
            id: Uuid::new_v4().to_string(),
            kind: WindowKind::Recurring,
            day_of_week: Some(1),
            date: None,
            start_minute: 9 * 60,
            end_minute: 12 * 60,
            label: "Morning".to_string(),
        }];
        snapshot.bootstrap.blocked_intervals = Vec::new();
        snapshot.bootstrap.capacity_overrides = Vec::<CapacityOverride>::new();
        save_bootstrap(app.clone(), &snapshot.bootstrap).await;

        generate(app.clone(), start).await;
        generate(app.clone(), start).await;

        let snapshot = get_snapshot(app).await;
        assert!(snapshot.current_schedule.is_some());
        assert!(snapshot.previous_schedule.is_some());
        assert_eq!(snapshot.current_schedule.unwrap().sessions.len(), 1);
    }

    #[tokio::test]
    async fn settings_endpoint_updates_snapshot_defaults() {
        let app = router(db::open_memory_database().expect("database"));
        let mut snapshot = get_snapshot(app.clone()).await;
        snapshot.bootstrap.settings.default_daily_topic_cap = 5;
        snapshot.bootstrap.settings.default_daily_cap_minutes = Some(150);
        snapshot.bootstrap.settings.priority_weights.neglect = 0.35;

        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/settings")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&snapshot.bootstrap.settings).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body: AppSnapshot = read_json(response).await;
        assert_eq!(body.bootstrap.settings.default_daily_topic_cap, 5);
        assert_eq!(body.bootstrap.settings.default_daily_cap_minutes, Some(150));
        assert_eq!(body.bootstrap.settings.priority_weights.neglect, 0.35);
    }

    #[tokio::test]
    async fn planner_simulation_returns_preview_without_persisting_schedule() {
        let app = router(db::open_memory_database().expect("database"));
        let start = NaiveDate::from_ymd_opt(2026, 6, 29).unwrap();
        let input = ScheduleInput {
            start_date: start,
            end_date: start,
            settings: AppSettings::default(),
            topics: vec![test_topic("linear", "Linear Algebra", 45, start)],
            study_windows: vec![AvailabilityWindow {
                id: Uuid::new_v4().to_string(),
                kind: WindowKind::Recurring,
                day_of_week: Some(1),
                date: None,
                start_minute: 9 * 60,
                end_minute: 12 * 60,
                label: "Morning".to_string(),
            }],
            blocked_intervals: Vec::new(),
            capacity_overrides: Vec::<CapacityOverride>::new(),
            last_studied_dates: std::collections::HashMap::new(),
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/planner/simulate")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&input).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let current_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/schedules/current")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let preview: SchedulePreview = read_json(response).await;
        let current: Option<PersistedScheduleRun> = read_json(current_response).await;
        assert_eq!(preview.sessions.len(), 1);
        assert!(current.is_none());
    }

    #[tokio::test]
    async fn pinning_current_schedule_lists_it_as_reference() {
        let app = router(db::open_memory_database().expect("database"));
        let mut snapshot = get_snapshot(app.clone()).await;
        let start = NaiveDate::from_ymd_opt(2026, 6, 29).unwrap();
        snapshot.bootstrap.settings.default_daily_topic_cap = 1;
        snapshot.bootstrap.topics = vec![test_topic("linear", "Linear Algebra", 45, start)];
        snapshot.bootstrap.study_windows = vec![AvailabilityWindow {
            id: Uuid::new_v4().to_string(),
            kind: WindowKind::Recurring,
            day_of_week: Some(1),
            date: None,
            start_minute: 9 * 60,
            end_minute: 12 * 60,
            label: "Morning".to_string(),
        }];
        snapshot.bootstrap.blocked_intervals = Vec::new();
        snapshot.bootstrap.capacity_overrides = Vec::<CapacityOverride>::new();
        save_bootstrap(app.clone(), &snapshot.bootstrap).await;
        generate(app.clone(), start).await;
        let current = get_snapshot(app.clone()).await.current_schedule.unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/schedules/{}/pin", current.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&PinScheduleRequest {
                            name: Some("Pinned baseline".to_string()),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let snapshot: AppSnapshot = read_json(response).await;
        assert!(snapshot.current_schedule.is_none());
        assert_eq!(snapshot.reference_schedules.len(), 1);
        assert_eq!(
            snapshot.reference_schedules[0].name.as_deref(),
            Some("Pinned baseline")
        );
    }

    #[tokio::test]
    async fn session_update_and_log_flow_updates_schedule_and_topic_progress() {
        let app = router(db::open_memory_database().expect("database"));
        let mut snapshot = get_snapshot(app.clone()).await;
        let start = NaiveDate::from_ymd_opt(2026, 6, 29).unwrap();
        snapshot.bootstrap.settings.default_daily_topic_cap = 1;
        snapshot.bootstrap.topics = vec![test_topic("linear", "Linear Algebra", 90, start)];
        snapshot.bootstrap.study_windows = vec![AvailabilityWindow {
            id: Uuid::new_v4().to_string(),
            kind: WindowKind::Recurring,
            day_of_week: Some(1),
            date: None,
            start_minute: 9 * 60,
            end_minute: 12 * 60,
            label: "Morning".to_string(),
        }];
        snapshot.bootstrap.blocked_intervals = Vec::new();
        snapshot.bootstrap.capacity_overrides = Vec::<CapacityOverride>::new();
        save_bootstrap(app.clone(), &snapshot.bootstrap).await;
        generate(app.clone(), start).await;
        let snapshot = get_snapshot(app.clone()).await;
        let session = snapshot.current_schedule.unwrap().sessions[0].clone();

        let update_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/sessions/{}", session.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&SessionUpdateRequest {
                            date: start,
                            start_minute: 10 * 60,
                            end_minute: 11 * 60,
                            locked: true,
                            status: SessionStatus::Locked,
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(update_response.status(), StatusCode::OK);
        let updated_session: crate::models::PersistedSession = read_json(update_response).await;
        assert!(updated_session.locked);
        assert_eq!(updated_session.status, SessionStatus::Locked);

        let log_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/sessions/{}/log", session.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&StudyLogRequest {
                            topic_id: session.topic_id.clone(),
                            date: start,
                            minutes: 60,
                            note: Some("Finished".to_string()),
                            status: Some(SessionStatus::Complete),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(log_response.status(), StatusCode::OK);
        let snapshot: AppSnapshot = read_json(log_response).await;
        let topic = snapshot
            .bootstrap
            .topics
            .iter()
            .find(|topic| topic.id == session.topic_id)
            .unwrap();
        let logged_session = &snapshot.current_schedule.unwrap().sessions[0];
        assert_eq!(topic.completed_minutes, 60);
        assert_eq!(logged_session.status, SessionStatus::Complete);
    }

    #[tokio::test]
    async fn manual_log_endpoint_updates_topic_progress_without_schedule() {
        let app = router(db::open_memory_database().expect("database"));
        let snapshot = get_snapshot(app.clone()).await;
        let topic_id = snapshot.bootstrap.topics[0].id.clone();
        let start = NaiveDate::from_ymd_opt(2026, 6, 29).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/logs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&StudyLogRequest {
                            topic_id: topic_id.clone(),
                            date: start,
                            minutes: 25,
                            note: Some("Manual".to_string()),
                            status: Some(SessionStatus::Manual),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let snapshot: AppSnapshot = read_json(response).await;
        let topic = snapshot
            .bootstrap
            .topics
            .iter()
            .find(|topic| topic.id == topic_id)
            .unwrap();
        assert_eq!(topic.completed_minutes, 25);
        assert!(snapshot.current_schedule.is_none());
    }

    async fn get_snapshot(app: Router) -> AppSnapshot {
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/bootstrap")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        read_json(response).await
    }

    async fn save_bootstrap(app: Router, bootstrap: &BootstrapData) {
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/bootstrap")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(bootstrap).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    async fn generate(app: Router, start: NaiveDate) {
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/schedules/generate")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&GenerateScheduleRequest {
                            start_date: Some(start),
                            end_date: Some(start),
                            persist: Some(true),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    async fn read_json<T: for<'de> Deserialize<'de>>(response: Response) -> T {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn test_topic(id: &str, name: &str, target_minutes: i64, deadline: NaiveDate) -> Topic {
        Topic {
            id: id.to_string(),
            name: name.to_string(),
            members: Vec::new(),
            min_session_minutes: 45,
            target_minutes,
            deadline: Some(deadline),
            completed_minutes: 0,
            elo: 1000.0,
            core_weekly_sessions: 0,
            archived: false,
            active_focus_index: 0,
        }
    }
}
