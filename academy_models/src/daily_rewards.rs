use std::{fmt, str::FromStr};

use postgres_types::private::BytesMut;
use postgres_types::{FromSql, IsNull, ToSql, Type};

use chrono::{DateTime, NaiveDate, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::user::UserId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DailyRewardCategory {
    Arrival,
    Lecture,
    Practice,
    Lab,
}

impl DailyRewardCategory {
    pub const ALL: [Self; 4] = [Self::Arrival, Self::Lecture, Self::Practice, Self::Lab];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Arrival => "arrival",
            Self::Lecture => "lecture",
            Self::Practice => "practice",
            Self::Lab => "lab",
        }
    }
}

impl fmt::Display for DailyRewardCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Error)]
#[error("Unknown daily reward category: {0}")]
pub struct ParseDailyRewardCategoryError(String);

impl FromStr for DailyRewardCategory {
    type Err = ParseDailyRewardCategoryError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "arrival" => Ok(Self::Arrival),
            "lecture" => Ok(Self::Lecture),
            "practice" => Ok(Self::Practice),
            "lab" => Ok(Self::Lab),
            other => Err(ParseDailyRewardCategoryError(other.into())),
        }
    }
}

impl TryFrom<&str> for DailyRewardCategory {
    type Error = ParseDailyRewardCategoryError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

impl ToSql for DailyRewardCategory {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        if ty.name() != "daily_reward_category" {
            return Err(format!(
                "DailyRewardCategory does not support Postgres type {}.{}",
                ty.schema(),
                ty.name()
            )
            .into());
        }

        out.extend_from_slice(self.as_str().as_bytes());
        Ok(IsNull::No)
    }

    fn accepts(ty: &Type) -> bool {
        ty.name() == "daily_reward_category"
    }

    postgres_types::to_sql_checked!();
}

impl<'a> FromSql<'a> for DailyRewardCategory {
    fn from_sql(
        ty: &Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if ty.name() != "daily_reward_category" {
            return Err(format!(
                "DailyRewardCategory does not support Postgres type {}.{}",
                ty.schema(),
                ty.name()
            )
            .into());
        }

        let value = std::str::from_utf8(raw)?;
        DailyRewardCategory::from_str(value).map_err(|err| Box::new(err) as _)
    }

    fn accepts(ty: &Type) -> bool {
        ty.name() == "daily_reward_category"
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DailyRewardEntry {
    pub id: Uuid,
    pub user_id: UserId,
    pub date_utc: NaiveDate,
    pub category: DailyRewardCategory,
    pub coins: i32,
    pub first_detected_at: Option<DateTime<Utc>>,
    pub last_detected_at: Option<DateTime<Utc>>,
    pub claimable_since: Option<DateTime<Utc>>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub activity_sample: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DailyRewardEntry {
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.claimable_since.is_some() && self.claimed_at.is_none()
    }

    #[must_use]
    pub fn is_claimed(&self) -> bool {
        self.claimed_at.is_some()
    }
}
