use academy_models::{
    auth::{AccessToken, Login, RefreshToken},
    session::{DeviceName, Session, SessionId},
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
    /// Whether the session was authenticated with a second factor.
    /// Administrative endpoints require this.
    pub mfa_verified: bool,
}

impl From<Session> for ApiSession {
    fn from(value: Session) -> Self {
        Self {
            id: value.id,
            user_id: value.user_id,
            device_name: value.device_name,
            last_update: value.updated_at.timestamp(),
            mfa_verified: value.mfa_verified,
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
