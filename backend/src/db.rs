use std::path::Path;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

use crate::models::{
    AppSettings, AvailabilityWindow, BootstrapData, CapacityOverride, PersistedScheduleRun,
    PersistedSession, ScheduleRunStatus, SessionStatus, Topic, WindowKind,
};
use crate::priority::{EloUpdate, apply_elo_win};
use crate::scheduler::{SchedulePreview, SessionExplanation};

const SETTINGS_KEY: &str = "app_settings";
const SEEDED_TOPICS: &[(&str, &[&str], bool)] = &[
    (
        "Operating Systems / Linux",
        &["Operating Systems", "Linux"],
        true,
    ),
    ("Computer Architecture", &[], true),
    ("Signal Processing", &[], true),
    (
        "Probability & Statistics / Statistics for Research",
        &["Probability & Statistics", "Statistics for Research"],
        true,
    ),
    ("numpy / scipy", &["numpy", "scipy"], true),
    ("Biology", &[], false),
    ("Chemistry", &[], false),
    ("Electricity & Magnetism", &[], false),
    ("Electronics", &[], false),
    ("Zephyr / BLE", &["Zephyr", "BLE"], false),
    ("lara project", &[], false),
    ("chatlan project", &[], false),
    ("CUDA C++", &[], false),
    ("Linear Algebra", &[], false),
    ("Discrete Mathematics", &[], false),
    (
        "PC Building / Server Building",
        &["PC Building", "Server Building"],
        false,
    ),
];

pub fn open_database(path: impl AsRef<Path>) -> Result<Connection> {
    let conn = Connection::open(path.as_ref()).with_context(|| {
        format!(
            "failed to open SQLite database at {}",
            path.as_ref().display()
        )
    })?;
    initialize_database(&conn)?;
    Ok(conn)
}

pub fn open_memory_database() -> Result<Connection> {
    let conn = Connection::open_in_memory().context("failed to open in-memory SQLite database")?;
    initialize_database(&conn)?;
    Ok(conn)
}

pub fn initialize_database(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(SCHEMA)?;
    seed_settings(conn)?;
    seed_topics(conn)?;
    Ok(())
}

pub fn load_bootstrap(conn: &Connection) -> Result<BootstrapData> {
    Ok(BootstrapData {
        settings: load_settings(conn)?,
        topics: list_topics(conn)?,
        study_windows: list_windows(conn, "study_windows")?,
        blocked_intervals: list_windows(conn, "blocked_intervals")?,
        capacity_overrides: list_capacity_overrides(conn)?,
    })
}

pub fn load_settings(conn: &Connection) -> Result<AppSettings> {
    let json: Option<String> = conn
        .query_row(
            "select value_json from settings where key = ?1",
            [SETTINGS_KEY],
            |row| row.get(0),
        )
        .optional()?;

    match json {
        Some(value) => serde_json::from_str(&value).context("failed to deserialize app settings"),
        None => Ok(AppSettings::default()),
    }
}

pub fn save_settings(conn: &Connection, settings: &AppSettings) -> Result<()> {
    let value = serde_json::to_string(settings).context("failed to serialize app settings")?;
    conn.execute(
        "insert into settings (key, value_json)
         values (?1, ?2)
         on conflict(key) do update set value_json = excluded.value_json, updated_at = current_timestamp",
        params![SETTINGS_KEY, value],
    )?;
    Ok(())
}

pub fn list_topics(conn: &Connection) -> Result<Vec<Topic>> {
    let mut stmt = conn.prepare(
        "select id, name, min_session_minutes, target_minutes, deadline, completed_minutes,
                elo, core_weekly_sessions, archived, active_focus_index
         from topics
         order by created_order asc",
    )?;

    let rows = stmt.query_map([], |row| topic_from_row(conn, row))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to load topics")
}

pub fn list_windows(conn: &Connection, table: &str) -> Result<Vec<AvailabilityWindow>> {
    let sql = match table {
        "study_windows" => {
            "select id, kind, day_of_week, date, start_minute, end_minute, label
             from study_windows
             order by coalesce(date, ''), coalesce(day_of_week, 99), start_minute"
        }
        "blocked_intervals" => {
            "select id, kind, day_of_week, date, start_minute, end_minute, label
             from blocked_intervals
             order by coalesce(date, ''), coalesce(day_of_week, 99), start_minute"
        }
        _ => anyhow::bail!("unsupported window table: {table}"),
    };

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], window_from_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to load availability windows")
}

pub fn list_capacity_overrides(conn: &Connection) -> Result<Vec<CapacityOverride>> {
    let mut stmt = conn.prepare(
        "select id, date, daily_cap_minutes, topic_cap
         from capacity_overrides
         order by date",
    )?;
    let rows = stmt.query_map([], capacity_override_from_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to load capacity overrides")
}

pub fn save_bootstrap(conn: &Connection, data: &BootstrapData) -> Result<BootstrapData> {
    save_settings(conn, &data.settings)?;
    upsert_topics(conn, &data.topics)?;
    replace_windows(conn, "study_windows", &data.study_windows)?;
    replace_windows(conn, "blocked_intervals", &data.blocked_intervals)?;
    replace_capacity_overrides(conn, &data.capacity_overrides)?;
    load_bootstrap(conn)
}

pub fn apply_priority_comparison(
    conn: &Connection,
    winner_topic_id: &str,
    loser_topic_id: &str,
    k_factor: f64,
) -> Result<EloUpdate> {
    let winner_before = topic_elo(conn, winner_topic_id)
        .with_context(|| format!("winner topic not found: {winner_topic_id}"))?;
    let loser_before = topic_elo(conn, loser_topic_id)
        .with_context(|| format!("loser topic not found: {loser_topic_id}"))?;
    let update = apply_elo_win(winner_before, loser_before, k_factor);

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "update topics set elo = ?1, updated_at = current_timestamp where id = ?2",
        params![update.winner_after, winner_topic_id],
    )?;
    tx.execute(
        "update topics set elo = ?1, updated_at = current_timestamp where id = ?2",
        params![update.loser_after, loser_topic_id],
    )?;
    tx.execute(
        "insert into priority_comparisons (
            id, winner_topic_id, loser_topic_id, winner_elo_before, loser_elo_before,
            winner_elo_after, loser_elo_after, k_factor
         )
         values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            Uuid::new_v4().to_string(),
            winner_topic_id,
            loser_topic_id,
            update.winner_before,
            update.loser_before,
            update.winner_after,
            update.loser_after,
            update.k_factor
        ],
    )?;
    tx.commit()?;
    Ok(update)
}

pub fn save_current_schedule(
    conn: &Connection,
    preview: &SchedulePreview,
) -> Result<PersistedScheduleRun> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "delete from schedule_runs where status = 'previous' and pinned = 0",
        [],
    )?;
    tx.execute(
        "update schedule_runs
         set status = 'previous'
         where status = 'current'",
        [],
    )?;

    let run_id = Uuid::new_v4().to_string();
    tx.execute(
        "insert into schedule_runs (
            id, status, name, start_date, end_date, pinned, feasibility_json
         )
         values (?1, 'current', null, ?2, ?3, 0, ?4)",
        params![
            run_id,
            preview.start_date.to_string(),
            preview.end_date.to_string(),
            serde_json::to_string(&preview.issues)?
        ],
    )?;

    for session in &preview.sessions {
        tx.execute(
            "insert into sessions (
                id, run_id, topic_id, focus_name, date, start_minute, end_minute,
                status, locked, explanation_json
             )
             values (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'planned', 0, ?8)",
            params![
                session.id,
                run_id,
                session.topic_id,
                session.focus_name,
                session.date.to_string(),
                session.start_minute,
                session.end_minute,
                serde_json::to_string(&session.explanation)?
            ],
        )?;
    }
    tx.commit()?;
    get_schedule_run(conn, &run_id)?.context("saved schedule run was not found")
}

pub fn get_schedule_by_status(
    conn: &Connection,
    status: ScheduleRunStatus,
) -> Result<Option<PersistedScheduleRun>> {
    let run_id: Option<String> = conn
        .query_row(
            "select id from schedule_runs where status = ?1 order by created_at desc limit 1",
            [status.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    run_id
        .map(|id| get_schedule_run(conn, &id))
        .transpose()
        .map(Option::flatten)
}

pub fn list_reference_schedules(conn: &Connection) -> Result<Vec<PersistedScheduleRun>> {
    let mut stmt = conn.prepare(
        "select id from schedule_runs where status = 'reference' order by created_at desc",
    )?;
    let ids = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ids.into_iter()
        .map(|id| get_schedule_run(conn, &id)?.context("reference schedule missing"))
        .collect()
}

pub fn pin_schedule_reference(conn: &Connection, run_id: &str, name: Option<&str>) -> Result<()> {
    conn.execute(
        "update schedule_runs
         set status = 'reference', pinned = 1, name = coalesce(?2, name)
         where id = ?1",
        params![run_id, name],
    )?;
    Ok(())
}

pub fn update_session(
    conn: &Connection,
    session_id: &str,
    date: NaiveDate,
    start_minute: i64,
    end_minute: i64,
    locked: bool,
    status: SessionStatus,
) -> Result<PersistedSession> {
    conn.execute(
        "update sessions
         set date = ?2, start_minute = ?3, end_minute = ?4, locked = ?5, status = ?6
         where id = ?1",
        params![
            session_id,
            date.to_string(),
            start_minute,
            end_minute,
            bool_to_int(locked),
            status.as_str()
        ],
    )?;
    get_session(conn, session_id)?.context("session not found after update")
}

pub fn log_session(
    conn: &Connection,
    session_id: Option<&str>,
    topic_id: &str,
    date: NaiveDate,
    minutes: i64,
    note: &str,
    status: Option<SessionStatus>,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "insert into study_logs (id, session_id, topic_id, date, minutes, note)
         values (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            Uuid::new_v4().to_string(),
            session_id,
            topic_id,
            date.to_string(),
            minutes,
            note
        ],
    )?;
    tx.execute(
        "update topics
         set completed_minutes = completed_minutes + ?1, updated_at = current_timestamp
         where id = ?2",
        params![minutes, topic_id],
    )?;
    if let (Some(id), Some(session_status)) = (session_id, status) {
        tx.execute(
            "update sessions set status = ?2 where id = ?1",
            params![id, session_status.as_str()],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn last_studied_dates(
    conn: &Connection,
) -> Result<std::collections::HashMap<String, NaiveDate>> {
    let mut stmt = conn.prepare("select topic_id, max(date) from study_logs group by topic_id")?;
    let rows = stmt.query_map([], |row| {
        let topic_id: String = row.get(0)?;
        let date_text: String = row.get(1)?;
        let date = NaiveDate::parse_from_str(&date_text, "%Y-%m-%d")
            .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;
        Ok((topic_id, date))
    })?;
    rows.collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()
        .context("failed to load last studied dates")
}

fn upsert_topics(conn: &Connection, topics: &[Topic]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for (index, topic) in topics.iter().enumerate() {
        tx.execute(
            "insert into topics (
                id, name, min_session_minutes, target_minutes, deadline, completed_minutes,
                elo, core_weekly_sessions, archived, active_focus_index, created_order
             )
             values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             on conflict(id) do update set
                name = excluded.name,
                min_session_minutes = excluded.min_session_minutes,
                target_minutes = excluded.target_minutes,
                deadline = excluded.deadline,
                completed_minutes = excluded.completed_minutes,
                elo = excluded.elo,
                core_weekly_sessions = excluded.core_weekly_sessions,
                archived = excluded.archived,
                active_focus_index = excluded.active_focus_index,
                created_order = excluded.created_order,
                updated_at = current_timestamp",
            params![
                topic.id,
                topic.name,
                topic.min_session_minutes,
                topic.target_minutes,
                topic.deadline.map(|date| date.to_string()),
                topic.completed_minutes,
                topic.elo,
                topic.core_weekly_sessions,
                bool_to_int(topic.archived),
                topic.active_focus_index,
                index as i64
            ],
        )?;
        tx.execute("delete from topic_members where topic_id = ?1", [&topic.id])?;
        for (position, member_name) in topic.members.iter().enumerate() {
            tx.execute(
                "insert into topic_members (id, topic_id, position, name)
                 values (?1, ?2, ?3, ?4)",
                params![
                    Uuid::new_v4().to_string(),
                    topic.id,
                    position as i64,
                    member_name
                ],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn replace_windows(conn: &Connection, table: &str, windows: &[AvailabilityWindow]) -> Result<()> {
    let table_name = match table {
        "study_windows" => "study_windows",
        "blocked_intervals" => "blocked_intervals",
        _ => anyhow::bail!("unsupported window table: {table}"),
    };
    let tx = conn.unchecked_transaction()?;
    tx.execute(&format!("delete from {table_name}"), [])?;
    for window in windows {
        tx.execute(
            &format!(
                "insert into {table_name} (
                    id, kind, day_of_week, date, start_minute, end_minute, label
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
            ),
            params![
                window.id,
                window.kind.as_str(),
                window.day_of_week,
                window.date.map(|date| date.to_string()),
                window.start_minute,
                window.end_minute,
                window.label
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn replace_capacity_overrides(conn: &Connection, overrides: &[CapacityOverride]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("delete from capacity_overrides", [])?;
    for override_value in overrides {
        tx.execute(
            "insert into capacity_overrides (id, date, daily_cap_minutes, topic_cap)
             values (?1, ?2, ?3, ?4)",
            params![
                override_value.id,
                override_value.date.to_string(),
                override_value.daily_cap_minutes,
                override_value.topic_cap
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn topic_elo(conn: &Connection, topic_id: &str) -> Result<f64> {
    conn.query_row("select elo from topics where id = ?1", [topic_id], |row| {
        row.get(0)
    })
    .optional()?
    .with_context(|| format!("topic not found: {topic_id}"))
}

fn get_schedule_run(conn: &Connection, run_id: &str) -> Result<Option<PersistedScheduleRun>> {
    let run = conn
        .query_row(
            "select id, status, name, start_date, end_date, pinned, feasibility_json
             from schedule_runs
             where id = ?1",
            [run_id],
            |row| {
                let start_date: String = row.get(3)?;
                let end_date: String = row.get(4)?;
                let issues_json: String = row.get(6)?;
                Ok((
                    row.get::<_, String>(0)?,
                    ScheduleRunStatus::from_str(row.get::<_, String>(1)?.as_str()),
                    row.get::<_, Option<String>>(2)?,
                    NaiveDate::parse_from_str(&start_date, "%Y-%m-%d")
                        .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?,
                    NaiveDate::parse_from_str(&end_date, "%Y-%m-%d")
                        .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?,
                    row.get::<_, i64>(5)? != 0,
                    issues_json,
                ))
            },
        )
        .optional()?;

    let Some((id, status, name, start_date, end_date, pinned, issues_json)) = run else {
        return Ok(None);
    };
    let issues = serde_json::from_str(&issues_json).context("failed to parse schedule issues")?;
    let sessions = list_sessions_for_run(conn, &id)?;
    Ok(Some(PersistedScheduleRun {
        id,
        status,
        name,
        start_date,
        end_date,
        pinned,
        issues,
        sessions,
    }))
}

fn list_sessions_for_run(conn: &Connection, run_id: &str) -> Result<Vec<PersistedSession>> {
    let mut stmt = conn.prepare(
        "select s.id, s.run_id, s.topic_id, t.name, s.focus_name, s.date, s.start_minute,
                s.end_minute, s.status, s.locked, s.explanation_json
         from sessions s
         join topics t on t.id = s.topic_id
         where s.run_id = ?1
         order by s.date, s.start_minute",
    )?;
    let rows = stmt.query_map([run_id], persisted_session_from_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to load sessions for schedule")
}

fn get_session(conn: &Connection, session_id: &str) -> Result<Option<PersistedSession>> {
    conn.query_row(
        "select s.id, s.run_id, s.topic_id, t.name, s.focus_name, s.date, s.start_minute,
                s.end_minute, s.status, s.locked, s.explanation_json
         from sessions s
         join topics t on t.id = s.topic_id
         where s.id = ?1",
        [session_id],
        persisted_session_from_row,
    )
    .optional()
    .context("failed to load session")
}

fn persisted_session_from_row(row: &Row<'_>) -> rusqlite::Result<PersistedSession> {
    let date_text: String = row.get(5)?;
    let explanation_json: String = row.get(10)?;
    let explanation: SessionExplanation = serde_json::from_str(&explanation_json)
        .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;
    Ok(PersistedSession {
        id: row.get(0)?,
        run_id: row.get(1)?,
        topic_id: row.get(2)?,
        topic_name: row.get(3)?,
        focus_name: row.get(4)?,
        date: NaiveDate::parse_from_str(&date_text, "%Y-%m-%d")
            .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?,
        start_minute: row.get(6)?,
        end_minute: row.get(7)?,
        status: SessionStatus::from_str(row.get::<_, String>(8)?.as_str()),
        locked: row.get::<_, i64>(9)? != 0,
        explanation,
    })
}

fn bool_to_int(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn seed_settings(conn: &Connection) -> Result<()> {
    if load_settings(conn).is_ok()
        && conn
            .query_row(
                "select exists(select 1 from settings where key = ?1)",
                [SETTINGS_KEY],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            == 1
    {
        return Ok(());
    }
    save_settings(conn, &AppSettings::default())
}

fn seed_topics(conn: &Connection) -> Result<()> {
    let topic_count: i64 = conn.query_row("select count(*) from topics", [], |row| row.get(0))?;
    if topic_count > 0 {
        return Ok(());
    }

    let tx = conn.unchecked_transaction()?;
    for (index, (name, members, boosted)) in SEEDED_TOPICS.iter().enumerate() {
        let topic_id = Uuid::new_v4().to_string();
        let elo = if *boosted { 1050.0 } else { 1000.0 };
        tx.execute(
            "insert into topics (
                id, name, min_session_minutes, target_minutes, deadline, completed_minutes,
                elo, core_weekly_sessions, archived, active_focus_index, created_order
             )
             values (?1, ?2, 45, 0, null, 0, ?3, 0, 0, 0, ?4)",
            params![topic_id, name, elo, index as i64],
        )?;

        for (position, member_name) in members.iter().enumerate() {
            tx.execute(
                "insert into topic_members (id, topic_id, position, name)
                 values (?1, ?2, ?3, ?4)",
                params![
                    Uuid::new_v4().to_string(),
                    topic_id,
                    position as i64,
                    member_name
                ],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn topic_from_row(conn: &Connection, row: &Row<'_>) -> rusqlite::Result<Topic> {
    let id: String = row.get(0)?;
    let deadline_text: Option<String> = row.get(4)?;
    let members = load_topic_members(conn, &id)?;

    Ok(Topic {
        id,
        name: row.get(1)?,
        members,
        min_session_minutes: row.get(2)?,
        target_minutes: row.get(3)?,
        deadline: deadline_text
            .as_deref()
            .map(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d"))
            .transpose()
            .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?,
        completed_minutes: row.get(5)?,
        elo: row.get(6)?,
        core_weekly_sessions: row.get(7)?,
        archived: row.get::<_, i64>(8)? != 0,
        active_focus_index: row.get(9)?,
    })
}

fn load_topic_members(conn: &Connection, topic_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("select name from topic_members where topic_id = ?1 order by position asc")?;
    let rows = stmt.query_map([topic_id], |row| row.get::<_, String>(0))?;
    rows.collect()
}

fn window_from_row(row: &Row<'_>) -> rusqlite::Result<AvailabilityWindow> {
    let date_text: Option<String> = row.get(3)?;
    Ok(AvailabilityWindow {
        id: row.get(0)?,
        kind: WindowKind::from_str(row.get::<_, String>(1)?.as_str()),
        day_of_week: row.get(2)?,
        date: date_text
            .as_deref()
            .map(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d"))
            .transpose()
            .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?,
        start_minute: row.get(4)?,
        end_minute: row.get(5)?,
        label: row.get(6)?,
    })
}

fn capacity_override_from_row(row: &Row<'_>) -> rusqlite::Result<CapacityOverride> {
    let date_text: String = row.get(1)?;
    Ok(CapacityOverride {
        id: row.get(0)?,
        date: NaiveDate::parse_from_str(&date_text, "%Y-%m-%d")
            .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?,
        daily_cap_minutes: row.get(2)?,
        topic_cap: row.get(3)?,
    })
}

const SCHEMA: &str = r#"
create table if not exists settings (
    key text primary key,
    value_json text not null,
    updated_at text not null default current_timestamp
);

create table if not exists topics (
    id text primary key,
    name text not null,
    min_session_minutes integer not null default 45 check (min_session_minutes > 0),
    target_minutes integer not null default 0 check (target_minutes >= 0),
    deadline text,
    completed_minutes integer not null default 0 check (completed_minutes >= 0),
    elo real not null default 1000.0,
    core_weekly_sessions integer not null default 0 check (core_weekly_sessions >= 0),
    archived integer not null default 0 check (archived in (0, 1)),
    active_focus_index integer not null default 0 check (active_focus_index >= 0),
    created_order integer not null default 0,
    created_at text not null default current_timestamp,
    updated_at text not null default current_timestamp
);

create table if not exists topic_members (
    id text primary key,
    topic_id text not null references topics(id) on delete cascade,
    position integer not null check (position >= 0),
    name text not null,
    unique(topic_id, position)
);

create table if not exists study_windows (
    id text primary key,
    kind text not null check (kind in ('recurring', 'one_off')),
    day_of_week integer check (day_of_week between 1 and 7),
    date text,
    start_minute integer not null check (start_minute >= 0 and start_minute < 1440),
    end_minute integer not null check (end_minute > 0 and end_minute <= 1440),
    label text not null default 'Study window',
    check (start_minute < end_minute),
    check ((kind = 'recurring' and day_of_week is not null and date is null)
        or (kind = 'one_off' and date is not null and day_of_week is null))
);

create table if not exists blocked_intervals (
    id text primary key,
    kind text not null check (kind in ('recurring', 'one_off')),
    day_of_week integer check (day_of_week between 1 and 7),
    date text,
    start_minute integer not null check (start_minute >= 0 and start_minute < 1440),
    end_minute integer not null check (end_minute > 0 and end_minute <= 1440),
    label text not null default 'Blocked',
    check (start_minute < end_minute),
    check ((kind = 'recurring' and day_of_week is not null and date is null)
        or (kind = 'one_off' and date is not null and day_of_week is null))
);

create table if not exists capacity_overrides (
    id text primary key,
    date text not null unique,
    daily_cap_minutes integer check (daily_cap_minutes is null or daily_cap_minutes >= 0),
    topic_cap integer check (topic_cap is null or topic_cap >= 0)
);

create table if not exists schedule_runs (
    id text primary key,
    status text not null check (status in ('current', 'previous', 'reference', 'simulation')),
    name text,
    start_date text not null,
    end_date text not null,
    created_at text not null default current_timestamp,
    pinned integer not null default 0 check (pinned in (0, 1)),
    feasibility_json text not null default '{}'
);

create table if not exists sessions (
    id text primary key,
    run_id text not null references schedule_runs(id) on delete cascade,
    topic_id text not null references topics(id),
    focus_name text not null,
    date text not null,
    start_minute integer not null check (start_minute >= 0 and start_minute < 1440),
    end_minute integer not null check (end_minute > 0 and end_minute <= 1440),
    status text not null default 'planned'
        check (status in ('planned', 'locked', 'complete', 'partial', 'missed', 'manual')),
    locked integer not null default 0 check (locked in (0, 1)),
    explanation_json text not null default '{}',
    check (start_minute < end_minute)
);

create table if not exists study_logs (
    id text primary key,
    session_id text references sessions(id) on delete set null,
    topic_id text not null references topics(id),
    date text not null,
    minutes integer not null check (minutes >= 0),
    note text not null default '',
    created_at text not null default current_timestamp
);

create table if not exists priority_comparisons (
    id text primary key,
    winner_topic_id text not null references topics(id),
    loser_topic_id text not null references topics(id),
    winner_elo_before real not null,
    loser_elo_before real not null,
    winner_elo_after real not null,
    loser_elo_after real not null,
    k_factor real not null,
    created_at text not null default current_timestamp
);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::{ScheduledSession, ScoreBreakdown};

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn explanation() -> SessionExplanation {
        SessionExplanation {
            score: 0.75,
            factors: ScoreBreakdown {
                preference: 1.0,
                urgency: 0.5,
                remaining: 0.25,
                core: 0.0,
                neglect: 0.0,
                pace: 0.0,
            },
            reason: "Test session".to_string(),
        }
    }

    fn preview_for(topic_id: &str, date: NaiveDate, start_minute: i64) -> SchedulePreview {
        SchedulePreview {
            can_generate: true,
            start_date: date,
            end_date: date,
            sessions: vec![ScheduledSession {
                id: Uuid::new_v4().to_string(),
                topic_id: topic_id.to_string(),
                topic_name: "Stored topic name is joined from topics".to_string(),
                focus_name: "Focus".to_string(),
                date,
                start_minute,
                end_minute: start_minute + 45,
                locked: false,
                explanation: explanation(),
            }],
            issues: Vec::new(),
        }
    }

    #[test]
    fn seeds_initial_topics_once() {
        let conn = open_memory_database().expect("database initializes");

        let topics = list_topics(&conn).expect("topics load");
        assert_eq!(topics.len(), 16);
        assert_eq!(topics[7].name, "Electricity & Magnetism");

        initialize_database(&conn).expect("reinitializes");
        let topics_after_second_init = list_topics(&conn).expect("topics load again");
        assert_eq!(topics_after_second_init.len(), 16);
    }

    #[test]
    fn boosts_first_five_topics_without_excluding_them() {
        let conn = open_memory_database().expect("database initializes");
        let topics = list_topics(&conn).expect("topics load");

        for topic in topics.iter().take(5) {
            assert_eq!(topic.elo, 1050.0);
            assert!(!topic.archived);
        }
        for topic in topics.iter().skip(5) {
            assert_eq!(topic.elo, 1000.0);
            assert!(!topic.archived);
        }
    }

    #[test]
    fn tuple_topics_keep_parent_targets_and_rotate_focus_labels() {
        let conn = open_memory_database().expect("database initializes");
        let topics = list_topics(&conn).expect("topics load");

        let os_linux = &topics[0];
        assert_eq!(os_linux.members, vec!["Operating Systems", "Linux"]);
        assert_eq!(os_linux.target_minutes, 0);
        assert_eq!(os_linux.focus_name(), "Operating Systems");
    }

    #[test]
    fn default_settings_match_mvp_policy() {
        let conn = open_memory_database().expect("database initializes");
        let settings = load_settings(&conn).expect("settings load");

        assert_eq!(settings.timezone, "America/Chicago");
        assert_eq!(settings.granularity_minutes, 15);
        assert_eq!(settings.week_start, "monday");
        assert_eq!(settings.planning_horizon_mode, "until_deadlines");
        assert_eq!(settings.priority_weights.preference, 0.25);
    }

    #[test]
    fn save_bootstrap_round_trips_settings_topics_windows_and_overrides() {
        let conn = open_memory_database().expect("database initializes");
        let mut bootstrap = load_bootstrap(&conn).expect("bootstrap loads");
        let override_date = date(2026, 7, 1);

        bootstrap.settings.default_daily_topic_cap = 4;
        bootstrap.settings.default_daily_cap_minutes = Some(180);
        bootstrap.topics[0].name = "Operating Systems tuple".to_string();
        bootstrap.topics[0].members = vec!["Scheduling".to_string(), "Memory".to_string()];
        bootstrap.topics[0].target_minutes = 240;
        bootstrap.topics[0].deadline = Some(date(2026, 8, 15));
        bootstrap.study_windows = vec![AvailabilityWindow {
            id: "study-window".to_string(),
            kind: WindowKind::Recurring,
            day_of_week: Some(3),
            date: None,
            start_minute: 18 * 60,
            end_minute: 20 * 60,
            label: "Evening".to_string(),
        }];
        bootstrap.blocked_intervals = vec![AvailabilityWindow {
            id: "blocked-window".to_string(),
            kind: WindowKind::OneOff,
            day_of_week: None,
            date: Some(override_date),
            start_minute: 19 * 60,
            end_minute: 20 * 60,
            label: "Dinner".to_string(),
        }];
        bootstrap.capacity_overrides = vec![CapacityOverride {
            id: "capacity".to_string(),
            date: override_date,
            daily_cap_minutes: Some(90),
            topic_cap: Some(2),
        }];

        let saved = save_bootstrap(&conn, &bootstrap).expect("bootstrap saves");
        let first_topic = &saved.topics[0];

        assert_eq!(saved.settings.default_daily_topic_cap, 4);
        assert_eq!(saved.settings.default_daily_cap_minutes, Some(180));
        assert_eq!(first_topic.name, "Operating Systems tuple");
        assert_eq!(first_topic.members, vec!["Scheduling", "Memory"]);
        assert_eq!(first_topic.target_minutes, 240);
        assert_eq!(first_topic.deadline, Some(date(2026, 8, 15)));
        assert_eq!(saved.study_windows, bootstrap.study_windows);
        assert_eq!(saved.blocked_intervals, bootstrap.blocked_intervals);
        assert_eq!(saved.capacity_overrides, bootstrap.capacity_overrides);
    }

    #[test]
    fn priority_comparison_updates_topics_and_records_history() {
        let conn = open_memory_database().expect("database initializes");
        let topics = list_topics(&conn).expect("topics load");
        let winner = &topics[0];
        let loser = &topics[5];

        let update = apply_priority_comparison(&conn, &winner.id, &loser.id, 24.0)
            .expect("comparison applies");
        let topics_after = list_topics(&conn).expect("topics reload");
        let winner_after = topics_after
            .iter()
            .find(|topic| topic.id == winner.id)
            .unwrap();
        let loser_after = topics_after
            .iter()
            .find(|topic| topic.id == loser.id)
            .unwrap();
        let comparison_count: i64 = conn
            .query_row("select count(*) from priority_comparisons", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(comparison_count, 1);
        assert_eq!(winner_after.elo, update.winner_after);
        assert_eq!(loser_after.elo, update.loser_after);
        assert!(winner_after.elo > winner.elo);
        assert!(loser_after.elo < loser.elo);
    }

    #[test]
    fn save_current_schedule_keeps_only_one_unpinned_previous() {
        let conn = open_memory_database().expect("database initializes");
        let topic_id = list_topics(&conn).expect("topics load")[0].id.clone();
        let first_date = date(2026, 6, 29);

        save_current_schedule(&conn, &preview_for(&topic_id, first_date, 9 * 60))
            .expect("first schedule saves");
        save_current_schedule(
            &conn,
            &preview_for(
                &topic_id,
                first_date.checked_add_days(chrono::Days::new(1)).unwrap(),
                10 * 60,
            ),
        )
        .expect("second schedule saves");
        save_current_schedule(
            &conn,
            &preview_for(
                &topic_id,
                first_date.checked_add_days(chrono::Days::new(2)).unwrap(),
                11 * 60,
            ),
        )
        .expect("third schedule saves");

        let previous_count: i64 = conn
            .query_row(
                "select count(*) from schedule_runs where status = 'previous'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let current = get_schedule_by_status(&conn, ScheduleRunStatus::Current)
            .expect("current loads")
            .expect("current exists");
        let previous = get_schedule_by_status(&conn, ScheduleRunStatus::Previous)
            .expect("previous loads")
            .expect("previous exists");

        assert_eq!(previous_count, 1);
        assert_eq!(current.start_date, date(2026, 7, 1));
        assert_eq!(previous.start_date, date(2026, 6, 30));
    }

    #[test]
    fn pin_schedule_reference_moves_run_out_of_current_history() {
        let conn = open_memory_database().expect("database initializes");
        let topic_id = list_topics(&conn).expect("topics load")[0].id.clone();
        let saved =
            save_current_schedule(&conn, &preview_for(&topic_id, date(2026, 6, 29), 9 * 60))
                .expect("schedule saves");

        pin_schedule_reference(&conn, &saved.id, Some("Baseline")).expect("schedule pins");

        let current = get_schedule_by_status(&conn, ScheduleRunStatus::Current).unwrap();
        let references = list_reference_schedules(&conn).expect("references load");
        assert!(current.is_none());
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].name.as_deref(), Some("Baseline"));
        assert!(references[0].pinned);
    }

    #[test]
    fn update_session_moves_time_and_sets_locked_status() {
        let conn = open_memory_database().expect("database initializes");
        let topic_id = list_topics(&conn).expect("topics load")[0].id.clone();
        let run = save_current_schedule(&conn, &preview_for(&topic_id, date(2026, 6, 29), 9 * 60))
            .expect("schedule saves");
        let session_id = run.sessions[0].id.clone();

        let updated = update_session(
            &conn,
            &session_id,
            date(2026, 6, 30),
            12 * 60,
            13 * 60,
            true,
            SessionStatus::Locked,
        )
        .expect("session updates");

        assert_eq!(updated.date, date(2026, 6, 30));
        assert_eq!(updated.start_minute, 12 * 60);
        assert_eq!(updated.end_minute, 13 * 60);
        assert!(updated.locked);
        assert_eq!(updated.status, SessionStatus::Locked);
    }

    #[test]
    fn log_session_updates_progress_status_and_last_studied_date() {
        let conn = open_memory_database().expect("database initializes");
        let topic_id = list_topics(&conn).expect("topics load")[0].id.clone();
        let study_date = date(2026, 6, 29);
        let run = save_current_schedule(&conn, &preview_for(&topic_id, study_date, 9 * 60))
            .expect("schedule saves");
        let session_id = run.sessions[0].id.clone();

        log_session(
            &conn,
            Some(&session_id),
            &topic_id,
            study_date,
            30,
            "Focused review",
            Some(SessionStatus::Partial),
        )
        .expect("session logs");

        let updated_topic = list_topics(&conn)
            .expect("topics reload")
            .into_iter()
            .find(|topic| topic.id == topic_id)
            .unwrap();
        let current = get_schedule_by_status(&conn, ScheduleRunStatus::Current)
            .expect("current loads")
            .unwrap();
        let last_studied = last_studied_dates(&conn).expect("last studied loads");
        let note: String = conn
            .query_row("select note from study_logs limit 1", [], |row| row.get(0))
            .unwrap();

        assert_eq!(updated_topic.completed_minutes, 30);
        assert_eq!(current.sessions[0].status, SessionStatus::Partial);
        assert_eq!(last_studied.get(&topic_id), Some(&study_date));
        assert_eq!(note, "Focused review");
    }
}
