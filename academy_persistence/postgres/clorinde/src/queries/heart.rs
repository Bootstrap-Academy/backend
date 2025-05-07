// This file was generated with `clorinde`. Do not modify.

#[derive(Clone, Copy, Debug)]
pub struct SetParams {
    pub user_id: uuid::Uuid,
    pub hearts: i64,
    pub last_refill: crate::types::time::TimestampTz,
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct Get {
    pub hearts: i64,
    pub last_refill: crate::types::time::TimestampTz,
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct GetQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> Result<Get, tokio_postgres::Error>,
    mapper: fn(Get) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> GetQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(Get) -> R) -> GetQuery<'c, 'a, 's, C, R, N> {
        GetQuery {
            client: self.client,
            params: self.params,
            stmt: self.stmt,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let stmt = self.stmt.prepare(self.client).await?;
        let row = self.client.query_one(stmt, &self.params).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let stmt = self.stmt.prepare(self.client).await?;
        Ok(self
            .client
            .query_opt(stmt, &self.params)
            .await?
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
        let stmt = self.stmt.prepare(self.client).await?;
        let it = self
            .client
            .query_raw(stmt, crate::slice_iter(&self.params))
            .await?
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(it)
    }
}
pub fn get() -> GetStmt {
    GetStmt(crate::client::async_::Stmt::new(
        "select hearts, last_refill from hearts where user_id=$1",
    ))
}
pub struct GetStmt(crate::client::async_::Stmt);
impl GetStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> GetQuery<'c, 'a, 's, C, Get, 1> {
        GetQuery {
            client,
            params: [user_id],
            stmt: &mut self.0,
            extractor: |row: &tokio_postgres::Row| -> Result<Get, tokio_postgres::Error> {
                Ok(Get {
                    hearts: row.try_get(0)?,
                    last_refill: row.try_get(1)?,
                })
            },
            mapper: |it| Get::from(it),
        }
    }
}
pub fn set() -> SetStmt {
    SetStmt(crate::client::async_::Stmt::new(
        "insert into hearts (user_id, hearts, last_refill) values ($1, $2, $3) on conflict (user_id) do update set hearts=$2, last_refill=$3",
    ))
}
pub struct SetStmt(crate::client::async_::Stmt);
impl SetStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
        hearts: &'a i64,
        last_refill: &'a crate::types::time::TimestampTz,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[user_id, hearts, last_refill]).await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SetParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SetStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a SetParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.user_id, &params.hearts, &params.last_refill))
    }
}
