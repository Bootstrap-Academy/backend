use academy_di::Build;
use academy_models::{
    oauth2::{OAuth2Link, OAuth2LinkId, OAuth2UserInfo},
    user::UserId,
};
use academy_persistence_contracts::oauth2::{OAuth2RepoError, OAuth2Repository};
use academy_utils::trace_instrument;
use bb8_postgres::tokio_postgres;
use clorinde::{
    client::Params,
    queries::{self, oauth2::CreateLinkParams},
};
use futures::{StreamExt, TryStreamExt};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresOAuth2Repository;

impl OAuth2Repository<PostgresTransaction> for PostgresOAuth2Repository {
    #[trace_instrument(skip(self, txn))]
    async fn list_links_by_user(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Vec<OAuth2Link>> {
        queries::oauth2::list_links_by_user()
            .bind(txn.txn(), &user_id)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_oauth2_link))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_link(
        &self,
        txn: &mut PostgresTransaction,
        link_id: OAuth2LinkId,
    ) -> anyhow::Result<Option<OAuth2Link>> {
        queries::oauth2::get_link()
            .bind(txn.txn(), &link_id)
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| row.map(decode_oauth2_link).transpose())
    }

    #[trace_instrument(skip(self, txn))]
    async fn create_link(
        &self,
        txn: &mut PostgresTransaction,
        oauth2_link: &OAuth2Link,
    ) -> Result<(), OAuth2RepoError> {
        let params = CreateLinkParams {
            id: *oauth2_link.id,
            user_id: *oauth2_link.user_id,
            provider_id: &*oauth2_link.provider_id,
            created_at: oauth2_link.created_at.into(),
            remote_user_id: &*oauth2_link.remote_user.id,
            remote_user_name: &*oauth2_link.remote_user.name,
        };

        queries::oauth2::create_link()
            .params(txn.txn(), &params)
            .await
            .map(|_| ())
            .map_err(map_oauth2_repo_error)
    }

    #[trace_instrument(skip(self, txn))]
    async fn delete_link(
        &self,
        txn: &mut PostgresTransaction,
        link_id: OAuth2LinkId,
    ) -> anyhow::Result<bool> {
        queries::oauth2::delete_link()
            .bind(txn.txn(), &link_id)
            .await
            .map(|n| n != 0)
            .map_err(Into::into)
    }
}

fn decode_oauth2_link(value: queries::oauth2::OAuth2Link) -> anyhow::Result<OAuth2Link> {
    Ok(OAuth2Link {
        id: value.id.into(),
        user_id: value.user_id.into(),
        provider_id: value.provider_id.into(),
        created_at: value.created_at.into(),
        remote_user: OAuth2UserInfo {
            id: value.remote_user_id.try_into()?,
            name: value.remote_user_name.try_into()?,
        },
    })
}

fn map_oauth2_repo_error(err: tokio_postgres::Error) -> OAuth2RepoError {
    match err.as_db_error() {
        Some(err) if err.constraint() == Some("oauth2_links_provider_id_remote_user_id_idx") => {
            OAuth2RepoError::Conflict
        }
        _ => OAuth2RepoError::Other(err.into()),
    }
}
