use std::path::Path;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

use crate::models::{
    AppSettings, AvailabilityWindow, BootstrapData, CapacityOverride, Topic, WindowKind,
};

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
}
