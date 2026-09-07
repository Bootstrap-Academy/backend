// This file was generated with `clorinde`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
> {
    pub id: uuid::Uuid,
    pub kind: crate::types::ContractDeclarationKind,
    pub received_at: chrono::DateTime<chrono::FixedOffset>,
    pub name: T1,
    pub email: T2,
    pub user_id: Option<uuid::Uuid>,
    pub contract: crate::types::ContractDeclarationContract,
    pub contract_designation: Option<T3>,
    pub cancellation_type: Option<crate::types::ContractCancellationType>,
    pub details: T4,
    pub requested_end: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub effective_end: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub processed_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub processing_note: Option<T5>,
}
#[derive(Clone, Copy, Debug)]
pub struct ListParams {
    pub kind: Option<crate::types::ContractDeclarationKind>,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug)]
pub struct SetProcessedParams<T1: crate::StringSql> {
    pub processed_at: chrono::DateTime<chrono::FixedOffset>,
    pub effective_end: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub processing_note: Option<T1>,
    pub id: uuid::Uuid,
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
    pub contract_designation: Option<String>,
    pub processing_note: Option<String>,
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
    pub contract_designation: Option<&'a str>,
    pub processing_note: Option<&'a str>,
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
            contract_designation,
            processing_note,
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
            contract_designation: contract_designation.map(|v| v.into()),
            processing_note: processing_note.map(|v| v.into()),
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
        "insert into contract_declarations (id, kind, received_at, name, email, user_id, contract, contract_designation, cancellation_type, details, requested_end, effective_end, processed_at, processing_note) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
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
        T4: crate::StringSql,
        T5: crate::StringSql,
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
        contract_designation: &'a Option<T3>,
        cancellation_type: &'a Option<crate::types::ContractCancellationType>,
        details: &'a T4,
        requested_end: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
        effective_end: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
        processed_at: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
        processing_note: &'a Option<T5>,
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
                    contract_designation,
                    cancellation_type,
                    details,
                    requested_end,
                    effective_end,
                    processed_at,
                    processing_note,
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
    T4: crate::StringSql,
    T5: crate::StringSql,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        CreateParams<T1, T2, T3, T4, T5>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CreateStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a CreateParams<T1, T2, T3, T4, T5>,
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
            &params.contract_designation,
            &params.cancellation_type,
            &params.details,
            &params.requested_end,
            &params.effective_end,
            &params.processed_at,
            &params.processing_note,
        ))
    }
}
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("select * from contract_declarations where id=$1", None)
}
impl GetStmt {
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
        id: &'a uuid::Uuid,
    ) -> ContractDeclarationQuery<'c, 'a, 's, C, ContractDeclaration, 1> {
        ContractDeclarationQuery {
            client,
            params: [id],
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
                    contract_designation: row.try_get(12)?,
                    processing_note: row.try_get(13)?,
                })
            },
            mapper: |it| ContractDeclaration::from(it),
        }
    }
}
pub struct ListStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list() -> ListStmt {
    ListStmt(
        "select * from contract_declarations where ($1::contract_declaration_kind is null or kind = $1) order by received_at desc, id desc limit $2 offset $3",
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
                    contract_designation: row.try_get(12)?,
                    processing_note: row.try_get(13)?,
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
                    contract_designation: row.try_get(12)?,
                    processing_note: row.try_get(13)?,
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
pub struct SetProcessedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_processed() -> SetProcessedStmt {
    SetProcessedStmt(
        "update contract_declarations set processed_at=$1, effective_end=$2, processing_note=$3 where id=$4 returning *",
        None,
    )
}
impl SetProcessedStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        processed_at: &'a chrono::DateTime<chrono::FixedOffset>,
        effective_end: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
        processing_note: &'a Option<T1>,
        id: &'a uuid::Uuid,
    ) -> ContractDeclarationQuery<'c, 'a, 's, C, ContractDeclaration, 4> {
        ContractDeclarationQuery {
            client,
            params: [processed_at, effective_end, processing_note, id],
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
                    contract_designation: row.try_get(12)?,
                    processing_note: row.try_get(13)?,
                })
            },
            mapper: |it| ContractDeclaration::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        SetProcessedParams<T1>,
        ContractDeclarationQuery<'c, 'a, 's, C, ContractDeclaration, 4>,
        C,
    > for SetProcessedStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a SetProcessedParams<T1>,
    ) -> ContractDeclarationQuery<'c, 'a, 's, C, ContractDeclaration, 4> {
        self.bind(
            client,
            &params.processed_at,
            &params.effective_end,
            &params.processing_note,
            &params.id,
        )
    }
}
pub struct DeleteByReceivedAtStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_by_received_at() -> DeleteByReceivedAtStmt {
    DeleteByReceivedAtStmt(
        "delete from contract_declarations where received_at<$1",
        None,
    )
}
impl DeleteByReceivedAtStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        received_at: &'a chrono::DateTime<chrono::FixedOffset>,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[received_at]).await
    }
}
