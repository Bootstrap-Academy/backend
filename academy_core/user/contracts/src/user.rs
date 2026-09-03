use std::future::Future;

use academy_models::{
    email_address::EmailAddress,
    oauth2::OAuth2Registration,
    pagination::PaginationSlice,
    user::{TermsVersion, UserComposite, UserDisplayName, UserFilter, UserName, UserPassword},
};
use thiserror::Error;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait UserService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Return a paginated and filtered list of all users.
    fn list(
        &self,
        txn: &mut Txn,
        query: UserListQuery,
    ) -> impl Future<Output = anyhow::Result<UserListResult>> + Send;

    /// Create a new user.
    fn create(
        &self,
        txn: &mut Txn,
        cmd: UserCreateCommand,
    ) -> impl Future<Output = Result<UserComposite, UserCreateError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserListQuery {
    pub pagination: PaginationSlice,
    pub filter: UserFilter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserListResult {
    pub total: u64,
    pub user_composites: Vec<UserComposite>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserCreateCommand {
    pub name: UserName,
    pub display_name: UserDisplayName,
    pub email: EmailAddress,
    pub password: Option<UserPassword>,
    pub admin: bool,
    pub enabled: bool,
    pub email_verified: bool,
    pub oauth2_registration: Option<OAuth2Registration>,
    /// Version of the terms and conditions the user accepted, if any.
    pub terms_version: Option<TermsVersion>,
    /// Whether the user confirmed to meet the minimum age.
    pub age_confirmed: bool,
}

#[derive(Debug, Error)]
pub enum UserCreateError {
    #[error("A user with the same name already exists.")]
    NameConflict,
    #[error("A user with the same email address already exists.")]
    EmailConflict,
    #[error("The remote user has already been linked.")]
    RemoteAlreadyLinked,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockUserService<Txn> {
    pub fn with_list(mut self, query: UserListQuery, result: UserListResult) -> Self {
        self.expect_list()
            .once()
            .with(mockall::predicate::always(), mockall::predicate::eq(query))
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_create(
        mut self,
        cmd: UserCreateCommand,
        result: Result<UserComposite, UserCreateError>,
    ) -> Self {
        self.expect_create()
            .once()
            .with(mockall::predicate::always(), mockall::predicate::eq(cmd))
            .return_once(|_, _| Box::pin(std::future::ready(result)));
        self
    }
}
