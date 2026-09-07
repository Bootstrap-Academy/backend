use std::future::Future;

use academy_models::{
    contract::{
        ContractDeclaration, ContractDeclarationId, ContractDeclarationKind, ContractProcessingNote,
    },
    pagination::PaginationSlice,
    user::UserId,
};
use chrono::{DateTime, Utc};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait ContractRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Create a new contract declaration.
    fn create(
        &self,
        txn: &mut Txn,
        declaration: ContractDeclaration,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Return the contract declaration with the given id.
    fn get(
        &self,
        txn: &mut Txn,
        id: ContractDeclarationId,
    ) -> impl Future<Output = anyhow::Result<Option<ContractDeclaration>>> + Send;

    /// Record that a declaration has been processed by hand and return the
    /// declaration as it is now stored.
    ///
    /// Returns `None` if there is no declaration with that id.
    fn set_processed(
        &self,
        txn: &mut Txn,
        id: ContractDeclarationId,
        processed_at: DateTime<Utc>,
        effective_end: Option<DateTime<Utc>>,
        processing_note: Option<ContractProcessingNote>,
    ) -> impl Future<Output = anyhow::Result<Option<ContractDeclaration>>> + Send;

    /// Return a paginated list of all contract declarations, most recent first.
    fn list(
        &self,
        txn: &mut Txn,
        kind: Option<ContractDeclarationKind>,
        pagination: PaginationSlice,
    ) -> impl Future<Output = anyhow::Result<Vec<ContractDeclaration>>> + Send;

    /// Return all contract declarations of the given user, oldest first.
    fn list_by_user_id(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Vec<ContractDeclaration>>> + Send;

    /// Return the total number of contract declarations.
    fn count(
        &self,
        txn: &mut Txn,
        kind: Option<ContractDeclarationKind>,
    ) -> impl Future<Output = anyhow::Result<u64>> + Send;

    /// Delete all declarations received before the given point in time and
    /// return how many were deleted.
    fn delete_by_received_at(
        &self,
        txn: &mut Txn,
        received_at: DateTime<Utc>,
    ) -> impl Future<Output = anyhow::Result<u64>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockContractRepository<Txn> {
    pub fn with_create(mut self, declaration: ContractDeclaration) -> Self {
        self.expect_create()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(declaration),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_get(
        mut self,
        id: ContractDeclarationId,
        result: Option<ContractDeclaration>,
    ) -> Self {
        self.expect_get()
            .once()
            .with(mockall::predicate::always(), mockall::predicate::eq(id))
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_set_processed(
        mut self,
        id: ContractDeclarationId,
        processed_at: DateTime<Utc>,
        effective_end: Option<DateTime<Utc>>,
        processing_note: Option<ContractProcessingNote>,
        result: Option<ContractDeclaration>,
    ) -> Self {
        self.expect_set_processed()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(id),
                mockall::predicate::eq(processed_at),
                mockall::predicate::eq(effective_end),
                mockall::predicate::eq(processing_note),
            )
            .return_once(move |_, _, _, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_list(
        mut self,
        kind: Option<ContractDeclarationKind>,
        pagination: PaginationSlice,
        result: Vec<ContractDeclaration>,
    ) -> Self {
        self.expect_list()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(kind),
                mockall::predicate::eq(pagination),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_list_by_user_id(
        mut self,
        user_id: UserId,
        result: Vec<ContractDeclaration>,
    ) -> Self {
        self.expect_list_by_user_id()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_count(mut self, kind: Option<ContractDeclarationKind>, result: u64) -> Self {
        self.expect_count()
            .once()
            .with(mockall::predicate::always(), mockall::predicate::eq(kind))
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_delete_by_received_at(mut self, received_at: DateTime<Utc>, result: u64) -> Self {
        self.expect_delete_by_received_at()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(received_at),
            )
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
