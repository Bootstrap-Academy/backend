use academy_core_oauth2_contracts::link::{OAuth2LinkService, OAuth2LinkServiceError};
use academy_di::Build;
use academy_models::{
    oauth2::{OAuth2Link, OAuth2ProviderId, OAuth2UserInfo},
    user::UserId,
};
use academy_persistence_contracts::oauth2::{OAuth2RepoError, OAuth2Repository};
use academy_shared_contracts::{id::IdService, time::TimeService};
use academy_utils::trace_instrument;

#[derive(Debug, Clone, Build)]
pub struct OAuth2LinkServiceImpl<Id, Time, OAuth2Repo> {
    id: Id,
    time: Time,
    oauth2_repo: OAuth2Repo,
}

impl<Txn, Id, Time, OAuth2Repo> OAuth2LinkService<Txn>
    for OAuth2LinkServiceImpl<Id, Time, OAuth2Repo>
where
    Txn: Send + Sync + 'static,
    Id: IdService,
    Time: TimeService,
    OAuth2Repo: OAuth2Repository<Txn>,
{
    #[trace_instrument(skip(self, txn, remote_user))]
    async fn create(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        provider_id: OAuth2ProviderId,
        remote_user: OAuth2UserInfo,
    ) -> Result<OAuth2Link, OAuth2LinkServiceError> {
        let link = OAuth2Link {
            id: self.id.generate(),
            user_id,
            provider_id,
            created_at: self.time.now(),
            remote_user,
        };

        self.oauth2_repo
            .create_link(txn, &link)
            .await
            .map_err(|err| match err {
                OAuth2RepoError::Conflict => OAuth2LinkServiceError::RemoteAlreadyLinked,
                OAuth2RepoError::Other(err) => err
                    .context("Failed to create OAuth2 link in database")
                    .into(),
            })?;

        Ok(link)
    }
}

#[cfg(test)]
mod tests {
    use academy_demo::{
        oauth2::{FOO_OAUTH2_LINK_1, TEST_OAUTH2_PROVIDER_ID},
        user::FOO,
    };
    use academy_persistence_contracts::oauth2::{MockOAuth2Repository, OAuth2RepoError};
    use academy_shared_contracts::{id::MockIdService, time::MockTimeService};
    use academy_utils::assert_matches;

    use super::*;

    #[tokio::test]
    async fn ok() {
        // Arrange
        let id = MockIdService::new().with_generate(FOO_OAUTH2_LINK_1.id);
        let time = MockTimeService::new().with_now(FOO_OAUTH2_LINK_1.created_at);

        let oauth2_repo =
            MockOAuth2Repository::new().with_create(FOO_OAUTH2_LINK_1.clone(), Ok(()));

        let sut = OAuth2LinkServiceImpl {
            id,
            time,
            oauth2_repo,
        };

        // Act
        let result = sut
            .create(
                &mut (),
                FOO.user.id,
                TEST_OAUTH2_PROVIDER_ID.clone(),
                FOO_OAUTH2_LINK_1.remote_user.clone(),
            )
            .await;

        // Assert
        assert_eq!(result.unwrap(), *FOO_OAUTH2_LINK_1);
    }

    #[tokio::test]
    async fn conflict() {
        // Arrange
        let id = MockIdService::new().with_generate(FOO_OAUTH2_LINK_1.id);
        let time = MockTimeService::new().with_now(FOO_OAUTH2_LINK_1.created_at);

        let oauth2_repo = MockOAuth2Repository::new()
            .with_create(FOO_OAUTH2_LINK_1.clone(), Err(OAuth2RepoError::Conflict));

        let sut = OAuth2LinkServiceImpl {
            id,
            time,
            oauth2_repo,
        };

        // Act
        let result = sut
            .create(
                &mut (),
                FOO.user.id,
                TEST_OAUTH2_PROVIDER_ID.clone(),
                FOO_OAUTH2_LINK_1.remote_user.clone(),
            )
            .await;

        // Assert
        assert_matches!(result, Err(OAuth2LinkServiceError::RemoteAlreadyLinked));
    }
}
