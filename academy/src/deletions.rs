//! Runs under academy-backend admission/hold, independently of payment and cache recovery.
use std::time::Duration;

use academy_extern_contracts::microservices::MicroservicesApiService;
use academy_persistence_contracts::{Database, Transaction};
use academy_persistence_postgres::{PostgresDatabase, deletion};
use tracing::warn;

pub async fn retry(
    db: &PostgresDatabase,
    services: &impl MicroservicesApiService,
) -> anyhow::Result<()> {
    // A fixed work budget and deferred failed rows prevent one bad recipient/service
    // from monopolizing a pass. Claims commit before HTTP, independently of ack.
    let mut attempted = Vec::new();
    let mut acknowledgement_failed = false;
    for _ in 0..100 {
        let mut txn = db.begin_transaction().await?;
        let Some(work) = deletion::claim(&mut txn, &attempted).await? else {
            break;
        };
        // No remote request unless its scheduling/ownership commit succeeds.
        txn.commit().await?;
        attempted.push(format!("{}:{}", *work.user_id, work.service));
        // Cap even a longer configured HTTP timeout below the 60-second lease.
        // A paused process can still outlive it; generation-fenced ack is required.
        let success = matches!(
            tokio::time::timeout(
                Duration::from_secs(30),
                services.delete_user_in_service(&work.service, work.user_id),
            )
            .await,
            Ok(Ok(()))
        );
        if !success {
            warn!(service=%work.service, attempts=work.attempts, since=%work.requested_at,
                "Account erasure delivery failed; durable work retained");
        }
        let result: anyhow::Result<()> = async {
            let mut txn = db.begin_transaction().await?;
            if !deletion::acknowledge(&mut txn, &work, success).await? {
                warn!(service=%work.service, "Erasure attempt superseded; leaving newer work intact");
            }
            txn.commit().await
        }
        .await;
        if result.is_err() {
            acknowledgement_failed = true;
            warn!(service=%work.service, "Erasure acknowledgement failed; retaining work and continuing pass");
        }
    }
    let mut txn = db.begin_transaction().await?;
    for (service, pending, oldest) in deletion::backlog(&mut txn).await? {
        warn!(%service, pending, %oldest, "Account erasure backlog requires monitoring");
    }
    if acknowledgement_failed {
        anyhow::bail!("Some erasure acknowledgements failed; pending work retained");
    }
    Ok(())
}
