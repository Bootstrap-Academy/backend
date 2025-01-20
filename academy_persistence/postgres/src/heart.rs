use academy_di::Build;
use academy_models::{heart::Hearts, user::UserId};
use academy_persistence_contracts::heart::HeartRepository;
use academy_utils::trace_instrument;
use clorinde::{
    client::Params,
    queries::{self, heart::SetParams},
};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresHeartRepository;

impl HeartRepository<PostgresTransaction> for PostgresHeartRepository {
    #[trace_instrument(skip(self, txn))]
    async fn get(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<Hearts>> {
        queries::heart::get()
            .bind(txn.txn(), &user_id)
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| {
                row.map(|row| {
                    Ok(Hearts {
                        hearts: row.hearts.try_into()?,
                        last_refill: row.last_refill.into(),
                    })
                })
                .transpose()
            })
    }

    #[trace_instrument(skip(self, txn))]
    async fn set(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        hearts: Hearts,
    ) -> anyhow::Result<()> {
        let params = SetParams {
            user_id: *user_id,
            hearts: hearts.hearts.try_into()?,
            last_refill: hearts.last_refill.into(),
        };

        queries::heart::set()
            .params(txn.txn(), &params)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}
