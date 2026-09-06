// This file was generated with `clorinde`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub id: uuid::Uuid,
    pub kind: crate::types::ContractDeclarationKind,
    pub received_at: chrono::DateTime<chrono::FixedOffset>,
    pub name: T1,
    pub email: T2,
    pub user_id: Option<uuid::Uuid>,
    pub contract: crate::types::ContractDeclarationContract,
    pub cancellation_type: Option<crate::types::ContractCancellationType>,
    pub details: T3,
    pub requested_end: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub effective_end: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub processed_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}
#[derive(Clone, Copy, Debug)]
pub struct ListParams {
    pub kind: Option<crate::types::ContractDeclarationKind>,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ContractDeclaration {
    pub id: uuid::Uuid,
    pub kind: crate::types::ContractDeclarationKind,
    pub received_at: chrono::DateTime<chrono::FixedOffset>,
    pub name: String,
    pub email: String,
    pub user_id: Option<uuid::Uuid>,
    pub contract: crate::types::ContractDeclarationContract,
    pub cancellation_type: Option<crate::types::ContractCancellationType>,
    pub details: String,
    pub requested_end: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub effective_end: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub processed_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}
pub struct ContractDeclarationBorrowed<'a> {
    pub id: uuid::Uuid,
    pub kind: crate::types::ContractDeclarationKind,
    pub received_at: chrono::DateTime<chrono::FixedOffset>,
    pub name: &'a str,
    pub email: &'a str,
    pub user_id: Option<uuid::Uuid>,
    pub contract: crate::types::ContractDeclarationContract,
    pub cancellation_type: Option<crate::types::ContractCancellationType>,
    pub details: &'a str,
    pub requested_end: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub effective_end: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub processed_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}
impl<'a> From<ContractDeclarationBorrowed<'a>> for ContractDeclaration {
    fn from(
        ContractDeclarationBorrowed {
            id,
            kind,
            received_at,
            name,
            email,
            user_id,
            contract,
            cancellation_type,
            details,
            requested_end,
            effective_end,
            processed_at,
        }: ContractDeclarationBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            kind,
            received_at,
            name: name.into(),
            email: email.into(),
            user_id,
            contract,
            cancellation_type,
            details: details.into(),
            requested_end,
            effective_end,
            processed_at,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct ContractDeclarationQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<ContractDeclarationBorrowed, tokio_postgres::Error>,
    mapper: fn(ContractDeclarationBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ContractDeclarationQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(ContractDeclarationBorrowed) -> R,
    ) -> ContractDeclarationQuery<'c, 'a, 's, C, R, N> {
        ContractDeclarationQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + use<'c, C, T, N>,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct I64Query<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<i64, tokio_postgres::Error>,
    mapper: fn(i64) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> I64Query<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(i64) -> R) -> I64Query<'c, 'a, 's, C, R, N> {
        I64Query {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + use<'c, C, T, N>,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct CreateStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn create() -> CreateStmt {
    CreateStmt(
        "insert into contract_declarations (id, kind, received_at, name, email, user_id, contract, cancellation_type, details, requested_end, effective_end, processed_at) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        None,
    )
}
impl CreateStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::StringSql,
        T2: crate::StringSql,
        T3: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        id: &'a uuid::Uuid,
        kind: &'a crate::types::ContractDeclarationKind,
        received_at: &'a chrono::DateTime<chrono::FixedOffset>,
        name: &'a T1,
        email: &'a T2,
        user_id: &'a Option<uuid::Uuid>,
        contract: &'a crate::types::ContractDeclarationContract,
        cancellation_type: &'a Option<crate::types::ContractCancellationType>,
        details: &'a T3,
        requested_end: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
        effective_end: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
        processed_at: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    id,
                    kind,
                    received_at,
                    name,
                    email,
                    user_id,
                    contract,
                    cancellation_type,
                    details,
                    requested_end,
                    effective_end,
                    processed_at,
                ],
            )
            .await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        CreateParams<T1, T2, T3>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CreateStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a CreateParams<T1, T2, T3>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.id,
            &params.kind,
            &params.received_at,
            &params.name,
            &params.email,
            &params.user_id,
            &params.contract,
            &params.cancellation_type,
            &params.details,
            &params.requested_end,
            &params.effective_end,
            &params.processed_at,
        ))
    }
}
pub struct ListStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list() -> ListStmt {
    ListStmt(
        "select * from contract_declarations where ($1::contract_declaration_kind is null or kind = $1) order by received_at desc limit $2 offset $3",
        None,
    )
}
impl ListStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        kind: &'a Option<crate::types::ContractDeclarationKind>,
        limit: &'a i64,
        offset: &'a i64,
    ) -> ContractDeclarationQuery<'c, 'a, 's, C, ContractDeclaration, 3> {
        ContractDeclarationQuery {
            client,
            params: [kind, limit, offset],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<ContractDeclarationBorrowed, tokio_postgres::Error> {
                Ok(ContractDeclarationBorrowed {
                    id: row.try_get(0)?,
                    kind: row.try_get(1)?,
                    received_at: row.try_get(2)?,
                    name: row.try_get(3)?,
                    email: row.try_get(4)?,
                    user_id: row.try_get(5)?,
                    contract: row.try_get(6)?,
                    cancellation_type: row.try_get(7)?,
                    details: row.try_get(8)?,
                    requested_end: row.try_get(9)?,
                    effective_end: row.try_get(10)?,
                    processed_at: row.try_get(11)?,
                })
            },
            mapper: |it| ContractDeclaration::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListParams,
        ContractDeclarationQuery<'c, 'a, 's, C, ContractDeclaration, 3>,
        C,
    > for ListStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListParams,
    ) -> ContractDeclarationQuery<'c, 'a, 's, C, ContractDeclaration, 3> {
        self.bind(client, &params.kind, &params.limit, &params.offset)
    }
}
pub struct ListByUserIdStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_by_user_id() -> ListByUserIdStmt {
    ListByUserIdStmt(
        "select * from contract_declarations where user_id=$1 order by received_at",
        None,
    )
}
impl ListByUserIdStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> ContractDeclarationQuery<'c, 'a, 's, C, ContractDeclaration, 1> {
        ContractDeclarationQuery {
            client,
            params: [user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<ContractDeclarationBorrowed, tokio_postgres::Error> {
                Ok(ContractDeclarationBorrowed {
                    id: row.try_get(0)?,
                    kind: row.try_get(1)?,
                    received_at: row.try_get(2)?,
                    name: row.try_get(3)?,
                    email: row.try_get(4)?,
                    user_id: row.try_get(5)?,
                    contract: row.try_get(6)?,
                    cancellation_type: row.try_get(7)?,
                    details: row.try_get(8)?,
                    requested_end: row.try_get(9)?,
                    effective_end: row.try_get(10)?,
                    processed_at: row.try_get(11)?,
                })
            },
            mapper: |it| ContractDeclaration::from(it),
        }
    }
}
pub struct CountStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count() -> CountStmt {
    CountStmt(
        "select count(*) from contract_declarations where ($1::contract_declaration_kind is null or kind = $1)",
        None,
    )
}
impl CountStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        kind: &'a Option<crate::types::ContractDeclarationKind>,
    ) -> I64Query<'c, 'a, 's, C, i64, 1> {
        I64Query {
            client,
            params: [kind],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
