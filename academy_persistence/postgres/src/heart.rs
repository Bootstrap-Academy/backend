use academy_di::Build;
use academy_models::{heart::Hearts, user::UserId};
use academy_persistence_contracts::heart::HeartRepository;

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresHeartRepository;

impl HeartRepository<PostgresTransaction> for PostgresHeartRepository {
    async fn get(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<Hearts>> {
        txn.txn()
            .query_opt(
                "select hearts, last_refill from hearts where user_id=$1",
                &[&*user_id],
            )
            .await
            .map_err(Into::into)
            .and_then(|row| {
                row.map(|row| {
                    Ok(Hearts {
                        hearts: row.get::<_, i64>(0).try_into()?,
                        last_refill: row.get(1),
                    })
                })
                .transpose()
            })
    }

    async fn set(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        hearts: Hearts,
    ) -> anyhow::Result<()> {
        txn.txn()
            .execute(
                "insert into hearts (user_id, hearts, last_refill) values ($1, $2, $3) on \
                 conflict (user_id) do update set hearts=$2, last_refill=$3",
                &[
                    &*user_id,
                    &i64::try_from(hearts.hearts)?,
                    &hearts.last_refill,
                ],
            )
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}
