use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriorityWeights {
    pub preference: f64,
    pub urgency: f64,
    pub remaining: f64,
    pub core: f64,
    pub neglect: f64,
    pub pace: f64,
}

impl Default for PriorityWeights {
    fn default() -> Self {
        Self {
            preference: 0.25,
            urgency: 0.25,
            remaining: 0.20,
            core: 0.15,
            neglect: 0.10,
            pace: 0.05,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub timezone: String,
    pub granularity_minutes: i64,
    pub week_start: String,
    pub default_daily_topic_cap: i64,
    pub default_daily_cap_minutes: Option<i64>,
    pub default_weekly_cap_minutes: Option<i64>,
    pub default_monthly_cap_minutes: Option<i64>,
    pub planning_horizon_mode: String,
    pub priority_weights: PriorityWeights,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            timezone: "America/Chicago".to_string(),
            granularity_minutes: 15,
            week_start: "monday".to_string(),
            default_daily_topic_cap: 3,
            default_daily_cap_minutes: None,
            default_weekly_cap_minutes: None,
            default_monthly_cap_minutes: None,
            planning_horizon_mode: "until_deadlines".to_string(),
            priority_weights: PriorityWeights::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    pub id: String,
    pub name: String,
    pub members: Vec<String>,
    pub min_session_minutes: i64,
    pub target_minutes: i64,
    pub deadline: Option<NaiveDate>,
    pub completed_minutes: i64,
    pub elo: f64,
    pub core_weekly_sessions: i64,
    pub archived: bool,
    pub active_focus_index: i64,
}

impl Topic {
    pub fn focus_name(&self) -> String {
        if self.members.is_empty() {
            self.name.clone()
        } else {
            let index = self
                .active_focus_index
                .rem_euclid(self.members.len() as i64) as usize;
            self.members[index].clone()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AvailabilityWindow {
    pub id: String,
    pub kind: WindowKind,
    pub day_of_week: Option<i64>,
    pub date: Option<NaiveDate>,
    pub start_minute: i64,
    pub end_minute: i64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WindowKind {
    Recurring,
    OneOff,
}

impl WindowKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Recurring => "recurring",
            Self::OneOff => "one_off",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "one_off" => Self::OneOff,
            _ => Self::Recurring,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CapacityOverride {
    pub id: String,
    pub date: NaiveDate,
    pub daily_cap_minutes: Option<i64>,
    pub topic_cap: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapData {
    pub settings: AppSettings,
    pub topics: Vec<Topic>,
    pub study_windows: Vec<AvailabilityWindow>,
    pub blocked_intervals: Vec<AvailabilityWindow>,
    pub capacity_overrides: Vec<CapacityOverride>,
}
