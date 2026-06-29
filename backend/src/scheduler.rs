use std::collections::HashMap;

use chrono::{Datelike, Days, NaiveDate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{
    AppSettings, AvailabilityWindow, CapacityOverride, PriorityWeights, Topic, WindowKind,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleInput {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub settings: AppSettings,
    pub topics: Vec<Topic>,
    pub study_windows: Vec<AvailabilityWindow>,
    pub blocked_intervals: Vec<AvailabilityWindow>,
    pub capacity_overrides: Vec<CapacityOverride>,
    pub last_studied_dates: HashMap<String, NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SchedulePreview {
    pub can_generate: bool,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub sessions: Vec<ScheduledSession>,
    pub issues: Vec<FeasibilityIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledSession {
    pub id: String,
    pub topic_id: String,
    pub topic_name: String,
    pub focus_name: String,
    pub date: NaiveDate,
    pub start_minute: i64,
    pub end_minute: i64,
    pub locked: bool,
    pub explanation: SessionExplanation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionExplanation {
    pub score: f64,
    pub factors: ScoreBreakdown,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScoreBreakdown {
    pub preference: f64,
    pub urgency: f64,
    pub remaining: f64,
    pub core: f64,
    pub neglect: f64,
    pub pace: f64,
}

impl ScoreBreakdown {
    fn weighted(&self, weights: &PriorityWeights) -> f64 {
        self.preference * weights.preference
            + self.urgency * weights.urgency
            + self.remaining * weights.remaining
            + self.core * weights.core
            + self.neglect * weights.neglect
            + self.pace * weights.pace
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeasibilityIssue {
    pub severity: IssueSeverity,
    pub code: String,
    pub message: String,
    pub date: Option<NaiveDate>,
    pub topic_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Blocker,
    Warning,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MinuteWindow {
    pub start_minute: i64,
    pub end_minute: i64,
}

impl MinuteWindow {
    fn duration(&self) -> i64 {
        self.end_minute - self.start_minute
    }
}

pub fn plan_schedule(input: ScheduleInput) -> SchedulePreview {
    let mut issues = missing_deadline_issues(&input.topics);
    if !issues.is_empty() {
        return SchedulePreview {
            can_generate: false,
            start_date: input.start_date,
            end_date: input.end_date,
            sessions: Vec::new(),
            issues,
        };
    }

    let active_topics: Vec<Topic> = input
        .topics
        .iter()
        .filter(|topic| !topic.archived)
        .cloned()
        .collect();
    let mut remaining_minutes: HashMap<String, i64> = active_topics
        .iter()
        .map(|topic| {
            (
                topic.id.clone(),
                (topic.target_minutes - topic.completed_minutes).max(0),
            )
        })
        .collect();
    let mut sessions = Vec::new();
    let mut planned_by_topic: HashMap<String, i64> = HashMap::new();
    let mut core_sessions_by_week: HashMap<(String, i32, u32), i64> = HashMap::new();
    let mut current = input.start_date;

    while current <= input.end_date {
        let mut free_windows = free_windows_for_date(
            current,
            &input.study_windows,
            &input.blocked_intervals,
            input.settings.granularity_minutes,
        );
        let capacity = capacity_for_date(current, &input.settings, &input.capacity_overrides);
        if let Some(cap) = capacity.daily_cap_minutes {
            free_windows = limit_windows_by_cap(&free_windows, cap);
        }

        if !free_windows.is_empty() && capacity.topic_cap > 0 {
            let mut scored_topics = active_topics
                .iter()
                .filter_map(|topic| {
                    let has_remaining = remaining_minutes.get(&topic.id).copied().unwrap_or(0) > 0;
                    let needs_core = core_shortfall(topic, current, &core_sessions_by_week) > 0.0;
                    if has_remaining || needs_core {
                        let explanation = score_topic(
                            topic,
                            current,
                            &input,
                            &remaining_minutes,
                            &core_sessions_by_week,
                        );
                        Some((topic, explanation))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            scored_topics.sort_by(|(_, left), (_, right)| {
                right
                    .score
                    .partial_cmp(&left.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            for (topic, explanation) in scored_topics
                .into_iter()
                .take(capacity.topic_cap.max(0) as usize)
            {
                let session_minutes = round_up(
                    topic.min_session_minutes,
                    input.settings.granularity_minutes,
                );
                let Some((start_minute, end_minute)) =
                    take_first_fit(&mut free_windows, session_minutes)
                else {
                    issues.push(FeasibilityIssue {
                        severity: IssueSeverity::Warning,
                        code: "no_window_for_min_session".to_string(),
                        message: format!(
                            "{} requires a {} minute session, but no remaining window fits on {}.",
                            topic.name, session_minutes, current
                        ),
                        date: Some(current),
                        topic_id: Some(topic.id.clone()),
                    });
                    continue;
                };

                let topic_session_count = planned_by_topic.get(&topic.id).copied().unwrap_or(0);
                let focus_name = focus_name_for_scheduled_session(topic, topic_session_count);
                sessions.push(ScheduledSession {
                    id: Uuid::new_v4().to_string(),
                    topic_id: topic.id.clone(),
                    topic_name: topic.name.clone(),
                    focus_name,
                    date: current,
                    start_minute,
                    end_minute,
                    locked: false,
                    explanation,
                });

                *planned_by_topic.entry(topic.id.clone()).or_default() += 1;
                let week = iso_week_key(current);
                *core_sessions_by_week
                    .entry((topic.id.clone(), week.0, week.1))
                    .or_default() += 1;
                let entry = remaining_minutes.entry(topic.id.clone()).or_default();
                *entry = (*entry - session_minutes).max(0);
            }
        }

        current = current
            .checked_add_days(Days::new(1))
            .expect("date range should remain representable");
    }

    for topic in &active_topics {
        let remaining = remaining_minutes.get(&topic.id).copied().unwrap_or(0);
        if remaining > 0 {
            issues.push(FeasibilityIssue {
                severity: IssueSeverity::Warning,
                code: "unmet_target_hours".to_string(),
                message: format!(
                    "{} remains {} minutes short by the planning horizon.",
                    topic.name, remaining
                ),
                date: topic.deadline,
                topic_id: Some(topic.id.clone()),
            });
        }
    }

    SchedulePreview {
        can_generate: true,
        start_date: input.start_date,
        end_date: input.end_date,
        sessions,
        issues,
    }
}

pub fn free_windows_for_date(
    date: NaiveDate,
    study_windows: &[AvailabilityWindow],
    blocked_intervals: &[AvailabilityWindow],
    granularity_minutes: i64,
) -> Vec<MinuteWindow> {
    let study = windows_matching_date(date, study_windows);
    let blocks = windows_matching_date(date, blocked_intervals);
    let mut free = study;
    for block in blocks {
        free = subtract_window(&free, block);
    }
    normalize_windows(free, granularity_minutes)
}

pub fn subtract_window(windows: &[MinuteWindow], block: MinuteWindow) -> Vec<MinuteWindow> {
    let mut result = Vec::new();
    for window in windows {
        if block.end_minute <= window.start_minute || block.start_minute >= window.end_minute {
            result.push(*window);
            continue;
        }
        if block.start_minute > window.start_minute {
            result.push(MinuteWindow {
                start_minute: window.start_minute,
                end_minute: block.start_minute.min(window.end_minute),
            });
        }
        if block.end_minute < window.end_minute {
            result.push(MinuteWindow {
                start_minute: block.end_minute.max(window.start_minute),
                end_minute: window.end_minute,
            });
        }
    }
    result
}

fn score_topic(
    topic: &Topic,
    date: NaiveDate,
    input: &ScheduleInput,
    remaining_minutes: &HashMap<String, i64>,
    core_sessions_by_week: &HashMap<(String, i32, u32), i64>,
) -> SessionExplanation {
    let active_topics: Vec<&Topic> = input
        .topics
        .iter()
        .filter(|candidate| !candidate.archived)
        .collect();
    let min_elo = active_topics
        .iter()
        .map(|candidate| candidate.elo)
        .fold(topic.elo, f64::min);
    let max_elo = active_topics
        .iter()
        .map(|candidate| candidate.elo)
        .fold(topic.elo, f64::max);
    let max_remaining = remaining_minutes
        .values()
        .copied()
        .max()
        .unwrap_or(0)
        .max(1) as f64;
    let remaining = remaining_minutes
        .get(&topic.id)
        .copied()
        .unwrap_or(0)
        .max(0) as f64;
    let days_until_deadline = topic
        .deadline
        .map(|deadline| (deadline - date).num_days())
        .unwrap_or(i64::MAX);
    let neglect_days = input
        .last_studied_dates
        .get(&topic.id)
        .map(|last| (date - *last).num_days().max(0) as f64)
        .unwrap_or(30.0);

    let factors = ScoreBreakdown {
        preference: normalize(topic.elo, min_elo, max_elo),
        urgency: deadline_urgency(days_until_deadline),
        remaining: (remaining / max_remaining).clamp(0.0, 1.0),
        core: core_shortfall(topic, date, core_sessions_by_week),
        neglect: (neglect_days / 30.0).clamp(0.0, 1.0),
        pace: pace_pressure(topic, date, input.start_date, remaining as i64),
    };
    let score = factors.weighted(&input.settings.priority_weights);
    SessionExplanation {
        score,
        factors,
        reason: format!("Selected by hybrid priority score {:.3}.", score),
    }
}

fn missing_deadline_issues(topics: &[Topic]) -> Vec<FeasibilityIssue> {
    topics
        .iter()
        .filter(|topic| {
            !topic.archived && topic.target_minutes > topic.completed_minutes && topic.deadline.is_none()
        })
        .map(|topic| FeasibilityIssue {
            severity: IssueSeverity::Blocker,
            code: "missing_deadline".to_string(),
            message: format!(
                "{} has remaining target hours but no deadline. Add a deadline before generating a schedule.",
                topic.name
            ),
            date: None,
            topic_id: Some(topic.id.clone()),
        })
        .collect()
}

fn windows_matching_date(date: NaiveDate, windows: &[AvailabilityWindow]) -> Vec<MinuteWindow> {
    let day = date.weekday().number_from_monday() as i64;
    windows
        .iter()
        .filter(|window| match window.kind {
            WindowKind::Recurring => window.day_of_week == Some(day),
            WindowKind::OneOff => window.date == Some(date),
        })
        .map(|window| MinuteWindow {
            start_minute: window.start_minute,
            end_minute: window.end_minute,
        })
        .collect()
}

fn normalize_windows(
    mut windows: Vec<MinuteWindow>,
    granularity_minutes: i64,
) -> Vec<MinuteWindow> {
    windows.sort_by_key(|window| (window.start_minute, window.end_minute));
    windows
        .into_iter()
        .filter_map(|window| {
            let start = round_up(window.start_minute, granularity_minutes);
            let end = round_down(window.end_minute, granularity_minutes);
            (start < end).then_some(MinuteWindow {
                start_minute: start,
                end_minute: end,
            })
        })
        .collect()
}

fn take_first_fit(windows: &mut Vec<MinuteWindow>, minutes: i64) -> Option<(i64, i64)> {
    for index in 0..windows.len() {
        if windows[index].duration() >= minutes {
            let start = windows[index].start_minute;
            let end = start + minutes;
            windows[index].start_minute = end;
            if windows[index].start_minute == windows[index].end_minute {
                windows.remove(index);
            }
            return Some((start, end));
        }
    }
    None
}

fn limit_windows_by_cap(windows: &[MinuteWindow], cap: i64) -> Vec<MinuteWindow> {
    let mut remaining = cap.max(0);
    let mut result = Vec::new();
    for window in windows {
        if remaining <= 0 {
            break;
        }
        let duration = window.duration().min(remaining);
        if duration > 0 {
            result.push(MinuteWindow {
                start_minute: window.start_minute,
                end_minute: window.start_minute + duration,
            });
            remaining -= duration;
        }
    }
    result
}

#[derive(Debug, Clone, Copy)]
struct DayCapacity {
    daily_cap_minutes: Option<i64>,
    topic_cap: i64,
}

fn capacity_for_date(
    date: NaiveDate,
    settings: &AppSettings,
    overrides: &[CapacityOverride],
) -> DayCapacity {
    let override_for_date = overrides.iter().find(|value| value.date == date);
    DayCapacity {
        daily_cap_minutes: override_for_date
            .and_then(|value| value.daily_cap_minutes)
            .or(settings.default_daily_cap_minutes),
        topic_cap: override_for_date
            .and_then(|value| value.topic_cap)
            .unwrap_or(settings.default_daily_topic_cap),
    }
}

fn focus_name_for_scheduled_session(topic: &Topic, topic_session_count: i64) -> String {
    if topic.members.is_empty() {
        topic.name.clone()
    } else {
        let index =
            (topic.active_focus_index + topic_session_count).rem_euclid(topic.members.len() as i64);
        topic.members[index as usize].clone()
    }
}

fn core_shortfall(
    topic: &Topic,
    date: NaiveDate,
    core_sessions_by_week: &HashMap<(String, i32, u32), i64>,
) -> f64 {
    if topic.core_weekly_sessions <= 0 {
        return 0.0;
    }
    let week = iso_week_key(date);
    let planned = core_sessions_by_week
        .get(&(topic.id.clone(), week.0, week.1))
        .copied()
        .unwrap_or(0);
    ((topic.core_weekly_sessions - planned).max(0) as f64 / topic.core_weekly_sessions as f64)
        .clamp(0.0, 1.0)
}

fn deadline_urgency(days_until_deadline: i64) -> f64 {
    if days_until_deadline == i64::MAX {
        return 0.0;
    }
    if days_until_deadline <= 0 {
        return 1.0;
    }
    (1.0 / (days_until_deadline as f64 + 1.0) * 14.0).clamp(0.0, 1.0)
}

fn pace_pressure(
    topic: &Topic,
    date: NaiveDate,
    start_date: NaiveDate,
    remaining_minutes: i64,
) -> f64 {
    let Some(deadline) = topic.deadline else {
        return 0.0;
    };
    if topic.target_minutes <= 0 {
        return 0.0;
    }
    let total_days = (deadline - start_date).num_days().max(1) as f64;
    let elapsed_days = (date - start_date).num_days().max(0) as f64;
    let expected_completed =
        topic.target_minutes as f64 * (elapsed_days / total_days).clamp(0.0, 1.0);
    let actual_or_planned_completed = (topic.target_minutes - remaining_minutes).max(0) as f64;
    ((expected_completed - actual_or_planned_completed).max(0.0) / topic.target_minutes as f64)
        .clamp(0.0, 1.0)
}

fn normalize(value: f64, min: f64, max: f64) -> f64 {
    if (max - min).abs() < f64::EPSILON {
        0.5
    } else {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    }
}

fn round_up(value: i64, granularity: i64) -> i64 {
    if granularity <= 1 {
        value
    } else {
        ((value + granularity - 1) / granularity) * granularity
    }
}

fn round_down(value: i64, granularity: i64) -> i64 {
    if granularity <= 1 {
        value
    } else {
        (value / granularity) * granularity
    }
}

fn iso_week_key(date: NaiveDate) -> (i32, u32) {
    let week = date.iso_week();
    (week.year(), week.week())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn topic(id: &str, name: &str, target_minutes: i64, deadline: Option<NaiveDate>) -> Topic {
        Topic {
            id: id.to_string(),
            name: name.to_string(),
            members: Vec::new(),
            min_session_minutes: 45,
            target_minutes,
            deadline,
            completed_minutes: 0,
            elo: 1000.0,
            core_weekly_sessions: 0,
            archived: false,
            active_focus_index: 0,
        }
    }

    fn settings_with_weights(priority_weights: PriorityWeights) -> AppSettings {
        AppSettings {
            default_daily_topic_cap: 1,
            priority_weights,
            ..AppSettings::default()
        }
    }

    fn only_weight(factor: &str) -> PriorityWeights {
        PriorityWeights {
            preference: if factor == "preference" { 1.0 } else { 0.0 },
            urgency: if factor == "urgency" { 1.0 } else { 0.0 },
            remaining: if factor == "remaining" { 1.0 } else { 0.0 },
            core: if factor == "core" { 1.0 } else { 0.0 },
            neglect: if factor == "neglect" { 1.0 } else { 0.0 },
            pace: if factor == "pace" { 1.0 } else { 0.0 },
        }
    }

    fn recurring_window(
        day_of_week: i64,
        start_minute: i64,
        end_minute: i64,
    ) -> AvailabilityWindow {
        AvailabilityWindow {
            id: Uuid::new_v4().to_string(),
            kind: WindowKind::Recurring,
            day_of_week: Some(day_of_week),
            date: None,
            start_minute,
            end_minute,
            label: "Window".to_string(),
        }
    }

    fn one_off_block(date: NaiveDate, start_minute: i64, end_minute: i64) -> AvailabilityWindow {
        AvailabilityWindow {
            id: Uuid::new_v4().to_string(),
            kind: WindowKind::OneOff,
            day_of_week: None,
            date: Some(date),
            start_minute,
            end_minute,
            label: "Block".to_string(),
        }
    }

    #[test]
    fn subtracts_blocked_time_from_study_windows() {
        let free = free_windows_for_date(
            date(2026, 6, 29),
            &[recurring_window(1, 9 * 60, 12 * 60)],
            &[one_off_block(date(2026, 6, 29), 10 * 60, 11 * 60)],
            15,
        );

        assert_eq!(
            free,
            vec![
                MinuteWindow {
                    start_minute: 9 * 60,
                    end_minute: 10 * 60
                },
                MinuteWindow {
                    start_minute: 11 * 60,
                    end_minute: 12 * 60
                }
            ]
        );
    }

    #[test]
    fn multiple_blocks_split_and_trim_free_windows_to_granularity() {
        let free = free_windows_for_date(
            date(2026, 6, 29),
            &[recurring_window(1, 8 * 60 + 7, 11 * 60 + 53)],
            &[
                one_off_block(date(2026, 6, 29), 8 * 60 + 30, 9 * 60 + 5),
                one_off_block(date(2026, 6, 29), 10 * 60 + 10, 10 * 60 + 40),
            ],
            15,
        );

        assert_eq!(
            free,
            vec![
                MinuteWindow {
                    start_minute: 8 * 60 + 15,
                    end_minute: 8 * 60 + 30
                },
                MinuteWindow {
                    start_minute: 9 * 60 + 15,
                    end_minute: 10 * 60
                },
                MinuteWindow {
                    start_minute: 10 * 60 + 45,
                    end_minute: 11 * 60 + 45
                }
            ]
        );
    }

    #[test]
    fn missing_deadline_blocks_generation_for_active_target_topic() {
        let preview = plan_schedule(ScheduleInput {
            start_date: date(2026, 6, 29),
            end_date: date(2026, 7, 5),
            settings: AppSettings::default(),
            topics: vec![topic("linear", "Linear Algebra", 120, None)],
            study_windows: vec![recurring_window(1, 9 * 60, 12 * 60)],
            blocked_intervals: Vec::new(),
            capacity_overrides: Vec::new(),
            last_studied_dates: HashMap::new(),
        });

        assert!(!preview.can_generate);
        assert_eq!(preview.issues[0].code, "missing_deadline");
        assert!(preview.sessions.is_empty());
    }

    #[test]
    fn archived_target_topics_do_not_block_generation_or_get_scheduled() {
        let start = date(2026, 6, 29);
        let mut archived = topic("archived", "Archived", 120, None);
        archived.archived = true;

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: start,
            settings: AppSettings::default(),
            topics: vec![topic("linear", "Linear Algebra", 45, Some(start)), archived],
            study_windows: vec![recurring_window(1, 9 * 60, 12 * 60)],
            blocked_intervals: Vec::new(),
            capacity_overrides: Vec::new(),
            last_studied_dates: HashMap::new(),
        });

        assert!(preview.can_generate);
        assert_eq!(preview.sessions.len(), 1);
        assert_eq!(preview.sessions[0].topic_id, "linear");
        assert!(
            preview
                .issues
                .iter()
                .all(|issue| issue.topic_id.as_deref() != Some("archived"))
        );
    }

    #[test]
    fn schedules_best_effort_sessions_inside_free_windows() {
        let start = date(2026, 6, 29);
        let mut settings = AppSettings::default();
        settings.default_daily_topic_cap = 2;

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: start,
            settings,
            topics: vec![
                topic("linear", "Linear Algebra", 45, Some(start)),
                topic("cuda", "CUDA C++", 45, Some(start)),
            ],
            study_windows: vec![recurring_window(1, 9 * 60, 11 * 60)],
            blocked_intervals: vec![one_off_block(start, 9 * 60 + 45, 10 * 60)],
            capacity_overrides: Vec::new(),
            last_studied_dates: HashMap::new(),
        });

        assert!(preview.can_generate);
        assert_eq!(preview.sessions.len(), 2);
        assert_eq!(preview.sessions[0].start_minute, 9 * 60);
        assert_eq!(preview.sessions[0].end_minute, 9 * 60 + 45);
        assert_eq!(preview.sessions[1].start_minute, 10 * 60);
        assert_eq!(preview.sessions[1].end_minute, 10 * 60 + 45);
    }

    #[test]
    fn daily_cap_limits_allocated_minutes() {
        let start = date(2026, 6, 29);
        let mut settings = AppSettings::default();
        settings.default_daily_topic_cap = 3;
        settings.default_daily_cap_minutes = Some(45);

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: start,
            settings,
            topics: vec![
                topic("linear", "Linear Algebra", 45, Some(start)),
                topic("cuda", "CUDA C++", 45, Some(start)),
            ],
            study_windows: vec![recurring_window(1, 9 * 60, 12 * 60)],
            blocked_intervals: Vec::new(),
            capacity_overrides: Vec::new(),
            last_studied_dates: HashMap::new(),
        });

        assert_eq!(preview.sessions.len(), 1);
        assert!(
            preview
                .issues
                .iter()
                .any(|issue| issue.code == "unmet_target_hours")
        );
    }

    #[test]
    fn capacity_override_can_raise_topic_cap_for_a_date() {
        let start = date(2026, 6, 29);
        let mut settings = AppSettings::default();
        settings.default_daily_topic_cap = 1;

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: start,
            settings,
            topics: vec![
                topic("linear", "Linear Algebra", 45, Some(start)),
                topic("cuda", "CUDA C++", 45, Some(start)),
            ],
            study_windows: vec![recurring_window(1, 9 * 60, 12 * 60)],
            blocked_intervals: Vec::new(),
            capacity_overrides: vec![CapacityOverride {
                id: "override".to_string(),
                date: start,
                daily_cap_minutes: None,
                topic_cap: Some(2),
            }],
            last_studied_dates: HashMap::new(),
        });

        assert_eq!(preview.sessions.len(), 2);
    }

    #[test]
    fn capacity_override_can_reduce_daily_minutes_for_a_date() {
        let start = date(2026, 6, 29);
        let mut settings = AppSettings::default();
        settings.default_daily_topic_cap = 3;
        settings.default_daily_cap_minutes = Some(3 * 60);

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: start,
            settings,
            topics: vec![
                topic("linear", "Linear Algebra", 45, Some(start)),
                topic("cuda", "CUDA C++", 45, Some(start)),
            ],
            study_windows: vec![recurring_window(1, 9 * 60, 12 * 60)],
            blocked_intervals: Vec::new(),
            capacity_overrides: vec![CapacityOverride {
                id: "override".to_string(),
                date: start,
                daily_cap_minutes: Some(45),
                topic_cap: Some(2),
            }],
            last_studied_dates: HashMap::new(),
        });

        assert_eq!(preview.sessions.len(), 1);
        assert_eq!(
            preview.sessions[0].end_minute - preview.sessions[0].start_minute,
            45
        );
    }

    #[test]
    fn min_session_length_rounds_up_to_granularity() {
        let start = date(2026, 6, 29);
        let mut rounded = topic("signals", "Signal Processing", 60, Some(start));
        rounded.min_session_minutes = 50;

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: start,
            settings: AppSettings::default(),
            topics: vec![rounded],
            study_windows: vec![recurring_window(1, 9 * 60, 12 * 60)],
            blocked_intervals: Vec::new(),
            capacity_overrides: Vec::new(),
            last_studied_dates: HashMap::new(),
        });

        assert_eq!(preview.sessions.len(), 1);
        assert_eq!(
            preview.sessions[0].end_minute - preview.sessions[0].start_minute,
            60
        );
    }

    #[test]
    fn no_available_windows_produces_unmet_target_warning() {
        let start = date(2026, 6, 29);

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: start,
            settings: AppSettings::default(),
            topics: vec![topic("linear", "Linear Algebra", 45, Some(start))],
            study_windows: Vec::new(),
            blocked_intervals: Vec::new(),
            capacity_overrides: Vec::new(),
            last_studied_dates: HashMap::new(),
        });

        assert!(preview.can_generate);
        assert!(preview.sessions.is_empty());
        assert!(
            preview
                .issues
                .iter()
                .any(|issue| issue.code == "unmet_target_hours")
        );
    }

    #[test]
    fn core_weekly_sessions_can_schedule_without_target_hours() {
        let start = date(2026, 6, 29);
        let mut core_topic = topic("os", "Operating Systems", 0, None);
        core_topic.core_weekly_sessions = 1;

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: start,
            settings: AppSettings::default(),
            topics: vec![core_topic],
            study_windows: vec![recurring_window(1, 9 * 60, 12 * 60)],
            blocked_intervals: Vec::new(),
            capacity_overrides: Vec::new(),
            last_studied_dates: HashMap::new(),
        });

        assert!(preview.can_generate);
        assert_eq!(preview.sessions.len(), 1);
        assert_eq!(preview.sessions[0].topic_id, "os");
    }

    #[test]
    fn core_weight_prioritizes_weekly_minimum_before_target_work() {
        let start = date(2026, 6, 29);
        let mut core_topic = topic("os", "Operating Systems", 0, None);
        core_topic.core_weekly_sessions = 1;

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: start,
            settings: settings_with_weights(only_weight("core")),
            topics: vec![
                topic("linear", "Linear Algebra", 45, Some(start)),
                core_topic,
            ],
            study_windows: vec![recurring_window(1, 9 * 60, 12 * 60)],
            blocked_intervals: Vec::new(),
            capacity_overrides: Vec::new(),
            last_studied_dates: HashMap::new(),
        });

        assert_eq!(preview.sessions.len(), 1);
        assert_eq!(preview.sessions[0].topic_id, "os");
        assert_eq!(preview.sessions[0].explanation.factors.core, 1.0);
    }

    #[test]
    fn preference_weight_prioritizes_higher_elo_topic() {
        let start = date(2026, 6, 29);
        let mut high = topic("cuda", "CUDA C++", 45, Some(start));
        high.elo = 1300.0;
        let mut low = topic("linear", "Linear Algebra", 45, Some(start));
        low.elo = 900.0;

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: start,
            settings: settings_with_weights(only_weight("preference")),
            topics: vec![low, high],
            study_windows: vec![recurring_window(1, 9 * 60, 12 * 60)],
            blocked_intervals: Vec::new(),
            capacity_overrides: Vec::new(),
            last_studied_dates: HashMap::new(),
        });

        assert_eq!(preview.sessions.len(), 1);
        assert_eq!(preview.sessions[0].topic_id, "cuda");
        assert_eq!(preview.sessions[0].explanation.factors.preference, 1.0);
    }

    #[test]
    fn urgency_weight_prioritizes_nearest_deadline() {
        let start = date(2026, 6, 29);
        let later = start.checked_add_days(Days::new(30)).unwrap();

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: start,
            settings: settings_with_weights(only_weight("urgency")),
            topics: vec![
                topic("later", "Later", 45, Some(later)),
                topic("today", "Today", 45, Some(start)),
            ],
            study_windows: vec![recurring_window(1, 9 * 60, 12 * 60)],
            blocked_intervals: Vec::new(),
            capacity_overrides: Vec::new(),
            last_studied_dates: HashMap::new(),
        });

        assert_eq!(preview.sessions.len(), 1);
        assert_eq!(preview.sessions[0].topic_id, "today");
        assert_eq!(preview.sessions[0].explanation.factors.urgency, 1.0);
    }

    #[test]
    fn remaining_weight_prioritizes_largest_remaining_target() {
        let start = date(2026, 6, 29);

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: start,
            settings: settings_with_weights(only_weight("remaining")),
            topics: vec![
                topic("small", "Small", 45, Some(start)),
                topic("large", "Large", 180, Some(start)),
            ],
            study_windows: vec![recurring_window(1, 9 * 60, 12 * 60)],
            blocked_intervals: Vec::new(),
            capacity_overrides: Vec::new(),
            last_studied_dates: HashMap::new(),
        });

        assert_eq!(preview.sessions.len(), 1);
        assert_eq!(preview.sessions[0].topic_id, "large");
        assert_eq!(preview.sessions[0].explanation.factors.remaining, 1.0);
    }

    #[test]
    fn neglect_weight_prioritizes_longest_unstudied_topic() {
        let start = date(2026, 6, 29);
        let neglected = start.checked_sub_days(Days::new(30)).unwrap();
        let recent = start.checked_sub_days(Days::new(1)).unwrap();

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: start,
            settings: settings_with_weights(only_weight("neglect")),
            topics: vec![
                topic("recent", "Recent", 45, Some(start)),
                topic("neglected", "Neglected", 45, Some(start)),
            ],
            study_windows: vec![recurring_window(1, 9 * 60, 12 * 60)],
            blocked_intervals: Vec::new(),
            capacity_overrides: Vec::new(),
            last_studied_dates: HashMap::from([
                ("recent".to_string(), recent),
                ("neglected".to_string(), neglected),
            ]),
        });

        assert_eq!(preview.sessions.len(), 1);
        assert_eq!(preview.sessions[0].topic_id, "neglected");
        assert_eq!(preview.sessions[0].explanation.factors.neglect, 1.0);
    }

    #[test]
    fn pace_weight_prioritizes_topic_that_is_furthest_behind() {
        let start = date(2026, 6, 29);
        let current = start.checked_add_days(Days::new(5)).unwrap();
        let deadline = start.checked_add_days(Days::new(10)).unwrap();
        let mut behind = topic("behind", "Behind", 600, Some(deadline));
        behind.completed_minutes = 0;
        let mut on_track = topic("track", "On Track", 600, Some(deadline));
        on_track.completed_minutes = 300;

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: current,
            settings: settings_with_weights(only_weight("pace")),
            topics: vec![on_track, behind],
            study_windows: vec![recurring_window(6, 9 * 60, 12 * 60)],
            blocked_intervals: Vec::new(),
            capacity_overrides: Vec::new(),
            last_studied_dates: HashMap::new(),
        });

        assert_eq!(preview.sessions.len(), 1);
        assert_eq!(preview.sessions[0].topic_id, "behind");
        assert!(preview.sessions[0].explanation.factors.pace > 0.0);
    }

    #[test]
    fn tuple_focus_rotates_by_scheduled_occurrence() {
        let start = date(2026, 6, 29);
        let mut settings = AppSettings::default();
        settings.default_daily_topic_cap = 1;
        let mut tuple = topic("os-linux", "Operating Systems / Linux", 90, Some(start));
        tuple.members = vec!["Operating Systems".to_string(), "Linux".to_string()];

        let preview = plan_schedule(ScheduleInput {
            start_date: start,
            end_date: start.checked_add_days(Days::new(1)).unwrap(),
            settings,
            topics: vec![tuple],
            study_windows: vec![
                recurring_window(1, 9 * 60, 12 * 60),
                recurring_window(2, 9 * 60, 12 * 60),
            ],
            blocked_intervals: Vec::new(),
            capacity_overrides: Vec::new(),
            last_studied_dates: HashMap::new(),
        });

        assert_eq!(preview.sessions.len(), 2);
        assert_eq!(preview.sessions[0].focus_name, "Operating Systems");
        assert_eq!(preview.sessions[1].focus_name, "Linux");
    }
}
