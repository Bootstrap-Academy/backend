use crate::PostgresTransaction;
use academy_di::Build;
use academy_models::user::UserId;
use academy_persistence_contracts::moderation::{
    CommercialPurchaseReadError, ModerationRepository,
};
use serde_json::Value;
#[derive(Debug, Clone, Build)]
pub struct PostgresModerationRepository;

async fn purchase_owner(
    txn: &mut PostgresTransaction,
    claimant: UserId,
    offer: uuid::Uuid,
) -> Result<UserId, CommercialPurchaseReadError> {
    let row = txn.txn().query_opt(
        "SELECT o.user_id,o.offer->>'id',o.offer->>'user_id' FROM purchase_offers o WHERE o.id=$1 AND commercial_owned_service_subject($2,o.user_id)",
        &[&offer, &*claimant],
    ).await.map_err(anyhow::Error::from)?
        .ok_or(CommercialPurchaseReadError::NotFound)?;
    let owner: uuid::Uuid = row.get(0);
    let original_id = row
        .get::<_, Option<String>>(1)
        .and_then(|value| uuid::Uuid::parse_str(&value).ok());
    let original_owner = row
        .get::<_, Option<String>>(2)
        .and_then(|value| uuid::Uuid::parse_str(&value).ok());
    if original_id != Some(offer) || original_owner != Some(owner) {
        return Err(CommercialPurchaseReadError::Unavailable);
    }
    Ok(owner.into())
}

impl ModerationRepository<PostgresTransaction> for PostgresModerationRepository {
    async fn commercial_case_subject(
        &self,
        txn: &mut PostgresTransaction,
        case_id: uuid::Uuid,
    ) -> anyhow::Result<Option<UserId>> {
        Ok(txn
            .txn()
            .query_opt(
                "SELECT subject FROM commercial_cases WHERE id=$1",
                &[&case_id],
            )
            .await?
            .map(|row| row.try_get::<_, uuid::Uuid>(0).map(UserId::from))
            .transpose()?)
    }
    async fn commercial_document_inventory(
        &self,
        txn: &mut PostgresTransaction,
        claimant: UserId,
    ) -> anyhow::Result<academy_models::commercial_document::DocumentInventory> {
        use academy_models::commercial_document::{DocumentInventory, InventoryScope};
        txn.txn()
            .batch_execute("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .await?;
        let header = txn.txn().query_one(
            "SELECT transaction_timestamp(), EXISTS(SELECT 1 FROM users WHERE id=$1) OR EXISTS(SELECT 1 FROM moderation_erasure_events WHERE subject=$1 AND retained_owner_inventory)",
            &[&*claimant],
        ).await?;
        let mut records = crate::purchase::document_inventory(txn, claimant).await?;
        records.extend(crate::finance::document_inventory(txn, claimant).await?);
        Ok(DocumentInventory {
            protocol: 1,
            claimant_subject: *claimant,
            observed_at: header.get(0),
            scope: InventoryScope {
                finance: "claimant_only".into(),
                purchases: "claimant_and_same_case_learning_subjects".into(),
                archives_scanned: false,
                remote_sources_queried: false,
                catalog_complete: false,
                historical_owner_inventory_complete: header.get(1),
                known_local_enumeration_complete: true,
            },
            records,
        })
    }
    async fn commercial_purchase_owner(
        &self,
        txn: &mut PostgresTransaction,
        claimant: UserId,
        offer: uuid::Uuid,
    ) -> Result<UserId, CommercialPurchaseReadError> {
        purchase_owner(txn, claimant, offer).await
    }

    async fn commercial_purchase_status(
        &self,
        txn: &mut PostgresTransaction,
        claimant: UserId,
        offer: uuid::Uuid,
    ) -> Result<academy_models::purchase::PurchaseStatus, CommercialPurchaseReadError> {
        let owner = purchase_owner(txn, claimant, offer).await?;
        crate::purchase::commercial_status(txn, offer, owner).await
    }
    async fn commercial_operation(
        &self,
        txn: &mut PostgresTransaction,
        operation: &str,
        actor: Option<UserId>,
        body: &Value,
    ) -> anyhow::Result<Value> {
        let body = body.to_string();
        let row = txn
            .txn()
            .query_one(
                "SELECT commercial_operation($1,$2,$3::text::jsonb)::text",
                &[&operation, &actor.map(|id| *id), &body],
            )
            .await
            .map_err(|error| -> anyhow::Error {
                if error.as_db_error().is_some_and(|e| {
                    ["P0001", "22P02", "22007", "23505", "23514"].contains(&e.code().code())
                }) {
                    academy_persistence_contracts::moderation::ModerationConflict.into()
                } else {
                    error.into()
                }
            })?;
        Ok(row
            .get::<_, Option<&str>>(0)
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or(Value::Null))
    }
    async fn operation(
        &self,
        txn: &mut PostgresTransaction,
        operation: &str,
        actor: Option<UserId>,
        body: &Value,
    ) -> anyhow::Result<Value> {
        let body = body.to_string();
        let row = txn
            .txn()
            .query_one(
                "SELECT backend_moderation($1,$2,$3::text::jsonb)::text",
                &[&operation, &actor.map(|id| *id), &body],
            )
            .await
            .map_err(|error| -> anyhow::Error {
                if error.as_db_error().is_some_and(|e| {
                    ["P0001", "22P02", "22007", "23505"].contains(&e.code().code())
                }) {
                    academy_persistence_contracts::moderation::ModerationConflict.into()
                } else {
                    error.into()
                }
            })?;
        Ok(serde_json::from_str(row.get::<_, &str>(0))?)
    }
}
