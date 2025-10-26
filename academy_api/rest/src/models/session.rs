use academy_models::{
    auth::{AccessToken, Login, RefreshToken},
    session::{ActiveUsersBucket, DeviceName, Session, SessionId},
    user::UserId,
};
use schemars::JsonSchema;
use serde::Serialize;

use super::user::ApiUser;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ApiSession {
    /// Session ID
    pub id: SessionId,
    /// User ID
    pub user_id: UserId,
    /// Device Name
    pub device_name: Option<DeviceName>,
    /// Timestamp of last refresh
    pub last_update: i64,
}

impl From<Session> for ApiSession {
    fn from(value: Session) -> Self {
        Self {
            id: value.id,
            user_id: value.user_id,
            device_name: value.device_name,
            last_update: value.updated_at.timestamp(),
        }
    }
}

#[derive(Serialize, JsonSchema)]
pub struct ApiLogin {
    user: ApiUser,
    session: ApiSession,
    access_token: AccessToken,
    refresh_token: RefreshToken,
}

impl From<Login> for ApiLogin {
    fn from(value: Login) -> Self {
        Self {
            user: value.user_composite.into(),
            session: value.session.into(),
            access_token: value.access_token,
            refresh_token: value.refresh_token,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ApiActiveUsersBucket {
    pub bucket_start: i64,
    pub active_users: u64,
}

impl From<ActiveUsersBucket> for ApiActiveUsersBucket {
    fn from(value: ActiveUsersBucket) -> Self {
        Self {
            bucket_start: value.bucket_start.timestamp(),
            active_users: value.active_users,
        }
    }
}
