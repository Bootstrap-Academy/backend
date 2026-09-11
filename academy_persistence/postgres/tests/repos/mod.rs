use academy_models::pagination::PaginationSlice;

mod admin_audit;
mod coins;
mod contract;
mod contract_lifecycle;
mod deletion;
mod finance;
mod heart;
mod mfa;
mod oauth2;
mod paypal;
mod premium;
mod session;
mod user;
mod withdrawal;

pub fn make_slice(limit: u64, offset: u64) -> PaginationSlice {
    PaginationSlice {
        limit: limit.try_into().unwrap(),
        offset,
    }
}

pub fn sliced<T>(data: &[T], slice: PaginationSlice) -> &[T] {
    let PaginationSlice { limit, offset } = slice;
    let limit = *limit as usize;
    let offset = offset as usize;

    &data[offset.min(data.len())..(offset + limit).min(data.len())]
}

/// Check the named evidence guard itself; a newer unconditional guard is not its witness.
pub async fn assert_down_refused(db: &crate::common::Db, name: &str, expected: &str) {
    use academy_persistence_contracts::{Database, Transaction};
    let migration = academy_persistence_postgres::MIGRATIONS
        .iter()
        .find(|m| m.name == name)
        .unwrap();
    let tx = db.begin_transaction().await.unwrap();
    let error = tx.txn().batch_execute(migration.down).await.unwrap_err();
    assert_eq!(error.code().unwrap().code(), "P0001");
    assert!(
        error.as_db_error().unwrap().message().contains(expected),
        "{error:?}"
    );
    tx.rollback().await.unwrap();
}

/// Count only actually installed historical migrations, never a later source suffix.
pub async fn revert_through(db: &crate::common::Db, name: &str) -> usize {
    let state = db.list_migrations().await.unwrap();
    let first = state.iter().position(|m| m.migration.name == name).unwrap();
    assert!(state[first].applied);
    state[first..].iter().filter(|m| m.applied).count()
}
