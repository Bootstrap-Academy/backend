// This file was generated with `clorinde`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub id: uuid::Uuid,
    pub at: chrono::DateTime<chrono::FixedOffset>,
    pub admin_user_id: uuid::Uuid,
    pub method: T1,
    pub path: T2,
    pub target_user_id: Option<uuid::Uuid>,
    pub status: i32,
    pub request_id: T3,
}
#[derive(Clone, Copy, Debug)]
pub struct ListParams {
    pub admin_user_id: Option<uuid::Uuid>,
    pub target_user_id: Option<uuid::Uuid>,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct CountParams {
    pub admin_user_id: Option<uuid::Uuid>,
    pub target_user_id: Option<uuid::Uuid>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct AdminAuditLogEntry {
    pub id: uuid::Uuid,
    pub at: chrono::DateTime<chrono::FixedOffset>,
    pub admin_user_id: uuid::Uuid,
    pub method: String,
    pub path: String,
    pub target_user_id: Option<uuid::Uuid>,
    pub status: i32,
    pub request_id: String,
}
pub struct AdminAuditLogEntryBorrowed<'a> {
    pub id: uuid::Uuid,
    pub at: chrono::DateTime<chrono::FixedOffset>,
    pub admin_user_id: uuid::Uuid,
    pub method: &'a str,
    pub path: &'a str,
    pub target_user_id: Option<uuid::Uuid>,
    pub status: i32,
    pub request_id: &'a str,
}
impl<'a> From<AdminAuditLogEntryBorrowed<'a>> for AdminAuditLogEntry {
    fn from(
        AdminAuditLogEntryBorrowed {
            id,
            at,
            admin_user_id,
            method,
            path,
            target_user_id,
            status,
            request_id,
        }: AdminAuditLogEntryBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            at,
            admin_user_id,
            method: method.into(),
            path: path.into(),
            target_user_id,
            status,
            request_id: request_id.into(),
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct AdminAuditLogEntryQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<AdminAuditLogEntryBorrowed, tokio_postgres::Error>,
    mapper: fn(AdminAuditLogEntryBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> AdminAuditLogEntryQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(AdminAuditLogEntryBorrowed) -> R,
    ) -> AdminAuditLogEntryQuery<'c, 'a, 's, C, R, N> {
        AdminAuditLogEntryQuery {
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
        "insert into admin_audit_log (id, at, admin_user_id, method, path, target_user_id, status, request_id) values ($1, $2, $3, $4, $5, $6, $7, $8)",
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
        at: &'a chrono::DateTime<chrono::FixedOffset>,
        admin_user_id: &'a uuid::Uuid,
        method: &'a T1,
        path: &'a T2,
        target_user_id: &'a Option<uuid::Uuid>,
        status: &'a i32,
        request_id: &'a T3,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    id,
                    at,
                    admin_user_id,
                    method,
                    path,
                    target_user_id,
                    status,
                    request_id,
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
            &params.at,
            &params.admin_user_id,
            &params.method,
            &params.path,
            &params.target_user_id,
            &params.status,
            &params.request_id,
        ))
    }
}
pub struct ListStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list() -> ListStmt {
    ListStmt(
        "select * from admin_audit_log where ($1::uuid is null or admin_user_id = $1) and ($2::uuid is null or target_user_id = $2) order by at desc, id desc limit $3 offset $4",
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
        admin_user_id: &'a Option<uuid::Uuid>,
        target_user_id: &'a Option<uuid::Uuid>,
        limit: &'a i64,
        offset: &'a i64,
    ) -> AdminAuditLogEntryQuery<'c, 'a, 's, C, AdminAuditLogEntry, 4> {
        AdminAuditLogEntryQuery {
            client,
            params: [admin_user_id, target_user_id, limit, offset],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<AdminAuditLogEntryBorrowed, tokio_postgres::Error> {
                Ok(AdminAuditLogEntryBorrowed {
                    id: row.try_get(0)?,
                    at: row.try_get(1)?,
                    admin_user_id: row.try_get(2)?,
                    method: row.try_get(3)?,
                    path: row.try_get(4)?,
                    target_user_id: row.try_get(5)?,
                    status: row.try_get(6)?,
                    request_id: row.try_get(7)?,
                })
            },
            mapper: |it| AdminAuditLogEntry::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListParams,
        AdminAuditLogEntryQuery<'c, 'a, 's, C, AdminAuditLogEntry, 4>,
        C,
    > for ListStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListParams,
    ) -> AdminAuditLogEntryQuery<'c, 'a, 's, C, AdminAuditLogEntry, 4> {
        self.bind(
            client,
            &params.admin_user_id,
            &params.target_user_id,
            &params.limit,
            &params.offset,
        )
    }
}
pub struct CountStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count() -> CountStmt {
    CountStmt(
        "select count(*) from admin_audit_log where ($1::uuid is null or admin_user_id = $1) and ($2::uuid is null or target_user_id = $2)",
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
        admin_user_id: &'a Option<uuid::Uuid>,
        target_user_id: &'a Option<uuid::Uuid>,
    ) -> I64Query<'c, 'a, 's, C, i64, 2> {
        I64Query {
            client,
            params: [admin_user_id, target_user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<'c, 'a, 's, CountParams, I64Query<'c, 'a, 's, C, i64, 2>, C>
    for CountStmt
{
    fn params(&'s self, client: &'c C, params: &'a CountParams) -> I64Query<'c, 'a, 's, C, i64, 2> {
        self.bind(client, &params.admin_user_id, &params.target_user_id)
    }
}
pub struct DeleteByAtStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_by_at() -> DeleteByAtStmt {
    DeleteByAtStmt("delete from admin_audit_log where at<$1", None)
}
impl DeleteByAtStmt {
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
        at: &'a chrono::DateTime<chrono::FixedOffset>,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[at]).await
    }
}
