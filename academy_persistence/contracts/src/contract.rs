use std::future::Future;

use academy_models::{
    contract::{ContractDeclaration, ContractDeclarationKind},
    pagination::PaginationSlice,
    user::UserId,
};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait ContractRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Create a new contract declaration.
    fn create(
        &self,
        txn: &mut Txn,
        declaration: ContractDeclaration,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

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
}
