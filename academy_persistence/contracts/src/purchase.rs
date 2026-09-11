use academy_models::purchase::{PurchaseRecord, PurchaseStatus};
use std::future::Future;
use uuid::Uuid;

pub trait PurchaseRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    fn observe_provision(
        &self,
        txn: &mut Txn,
        id: Uuid,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    fn timing_statement(
        &self,
        txn: &mut Txn,
        id: Uuid,
        original: bool,
    ) -> impl Future<Output = anyhow::Result<Option<Vec<u8>>>> + Send;
    fn timestamp(
        &self,
        txn: &mut Txn,
    ) -> impl Future<Output = anyhow::Result<chrono::DateTime<chrono::Utc>>> + Send;
    fn cash_capture(
        &self,
        txn: &mut Txn,
        id: Uuid,
        evidence: &str,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    fn invoice_artifact(
        &self,
        txn: &mut Txn,
        order: &str,
        candidate: &str,
    ) -> impl Future<Output = anyhow::Result<String>> + Send;
    fn invoice_attempt(
        &self,
        txn: &mut Txn,
        order: &str,
        attempt: Uuid,
        observation: &str,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    fn export(
        &self,
        txn: &mut Txn,
        user: Uuid,
    ) -> impl Future<Output = anyhow::Result<String>> + Send;
    fn record_debit(
        &self,
        txn: &mut Txn,
        id: Uuid,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    fn submit(
        &self,
        txn: &mut Txn,
        id: Uuid,
        payload: &str,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    fn bind_period(
        &self,
        txn: &mut Txn,
        id: Uuid,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;
    fn create(
        &self,
        txn: &mut Txn,
        record: &PurchaseRecord,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    fn get(
        &self,
        txn: &mut Txn,
        id: Uuid,
    ) -> impl Future<Output = anyhow::Result<Option<PurchaseRecord>>> + Send;
    fn lock_user(
        &self,
        txn: &mut Txn,
        user: Uuid,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;
    fn accept(
        &self,
        txn: &mut Txn,
        id: Uuid,
        body: &str,
        metadata: &str,
        accepted_at: chrono::DateTime<chrono::Utc>,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    fn state(
        &self,
        txn: &mut Txn,
        id: Uuid,
        state: &str,
        reason: Option<&str>,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    fn fulfill(
        &self,
        txn: &mut Txn,
        id: Uuid,
        result: &str,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    fn fulfillment_statement(
        &self,
        txn: &mut Txn,
        id: Uuid,
        original: bool,
    ) -> impl Future<Output = anyhow::Result<Option<Vec<u8>>>> + Send;
    fn pending(&self, txn: &mut Txn) -> impl Future<Output = anyhow::Result<Vec<Uuid>>> + Send;
    fn claim(
        &self,
        txn: &mut Txn,
        id: Uuid,
    ) -> impl Future<Output = anyhow::Result<Option<PurchaseRecord>>> + Send;
    fn acknowledge(
        &self,
        txn: &mut Txn,
        id: Uuid,
        generation: i64,
        outcome: &str,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    fn list(
        &self,
        txn: &mut Txn,
        user: Uuid,
    ) -> impl Future<Output = anyhow::Result<Vec<PurchaseStatus>>> + Send;
}
