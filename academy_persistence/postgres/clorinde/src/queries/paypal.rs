// This file was generated with `clorinde`. Do not modify.

#[derive(Debug)]
pub struct CreateCoinOrderParams<T1: crate::StringSql> {
    pub id: T1,
    pub user_id: uuid::Uuid,
    pub created_at: crate::types::time::TimestampTz,
    pub captured_at: Option<crate::types::time::TimestampTz>,
    pub coins: i64,
    pub invoice_number: i64,
}
#[derive(Debug)]
pub struct CaptureCoinOrderParams<T1: crate::StringSql> {
    pub captured_at: crate::types::time::TimestampTz,
    pub id: T1,
}
#[derive(Debug, Clone, PartialEq)]
pub struct CoinOrder {
    pub id: String,
    pub user_id: uuid::Uuid,
    pub created_at: crate::types::time::TimestampTz,
    pub captured_at: Option<crate::types::time::TimestampTz>,
    pub coins: i64,
    pub invoice_number: i64,
}
pub struct CoinOrderBorrowed<'a> {
    pub id: &'a str,
    pub user_id: uuid::Uuid,
    pub created_at: crate::types::time::TimestampTz,
    pub captured_at: Option<crate::types::time::TimestampTz>,
    pub coins: i64,
    pub invoice_number: i64,
}
impl<'a> From<CoinOrderBorrowed<'a>> for CoinOrder {
    fn from(
        CoinOrderBorrowed {
            id,
            user_id,
            created_at,
            captured_at,
            coins,
            invoice_number,
        }: CoinOrderBorrowed<'a>,
    ) -> Self {
        Self {
            id: id.into(),
            user_id,
            created_at,
            captured_at,
            coins,
            invoice_number,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct I64Query<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> i64,
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
            stmt: self.stmt,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let stmt = self.stmt.prepare(self.client).await?;
        let row = self.client.query_one(stmt, &self.params).await?;
        Ok((self.mapper)((self.extractor)(&row)))
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
            .map(|row| (self.mapper)((self.extractor)(&row))))
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stmt = self.stmt.prepare(self.client).await?;
        let it = self
            .client
            .query_raw(stmt, crate::slice_iter(&self.params))
            .await?
            .map(move |res| res.map(|row| (self.mapper)((self.extractor)(&row))))
            .into_stream();
        Ok(it)
    }
}
pub struct CoinOrderQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> CoinOrderBorrowed,
    mapper: fn(CoinOrderBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> CoinOrderQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(CoinOrderBorrowed) -> R) -> CoinOrderQuery<'c, 'a, 's, C, R, N> {
        CoinOrderQuery {
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
        Ok((self.mapper)((self.extractor)(&row)))
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
            .map(|row| (self.mapper)((self.extractor)(&row))))
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stmt = self.stmt.prepare(self.client).await?;
        let it = self
            .client
            .query_raw(stmt, crate::slice_iter(&self.params))
            .await?
            .map(move |res| res.map(|row| (self.mapper)((self.extractor)(&row))))
            .into_stream();
        Ok(it)
    }
}
pub fn create_coin_order() -> CreateCoinOrderStmt {
    CreateCoinOrderStmt(crate::client::async_::Stmt::new(
        "insert into paypal_coin_orders (id, user_id, created_at, captured_at, coins, invoice_number) values ($1, $2, $3, $4, $5, $6)",
    ))
}
pub struct CreateCoinOrderStmt(crate::client::async_::Stmt);
impl CreateCoinOrderStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s mut self,
        client: &'c C,
        id: &'a T1,
        user_id: &'a uuid::Uuid,
        created_at: &'a crate::types::time::TimestampTz,
        captured_at: &'a Option<crate::types::time::TimestampTz>,
        coins: &'a i64,
        invoice_number: &'a i64,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client
            .execute(
                stmt,
                &[id, user_id, created_at, captured_at, coins, invoice_number],
            )
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        CreateCoinOrderParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CreateCoinOrderStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a CreateCoinOrderParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.id,
            &params.user_id,
            &params.created_at,
            &params.captured_at,
            &params.coins,
            &params.invoice_number,
        ))
    }
}
pub fn count_coin_orders() -> CountCoinOrdersStmt {
    CountCoinOrdersStmt(crate::client::async_::Stmt::new(
        "select count(*) from paypal_coin_orders",
    ))
}
pub struct CountCoinOrdersStmt(crate::client::async_::Stmt);
impl CountCoinOrdersStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
    ) -> I64Query<'c, 'a, 's, C, i64, 0> {
        I64Query {
            client,
            params: [],
            stmt: &mut self.0,
            extractor: |row| row.get(0),
            mapper: |it| it,
        }
    }
}
pub fn list_coin_orders() -> ListCoinOrdersStmt {
    ListCoinOrdersStmt(crate::client::async_::Stmt::new(
        "select * from paypal_coin_orders",
    ))
}
pub struct ListCoinOrdersStmt(crate::client::async_::Stmt);
impl ListCoinOrdersStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
    ) -> CoinOrderQuery<'c, 'a, 's, C, CoinOrder, 0> {
        CoinOrderQuery {
            client,
            params: [],
            stmt: &mut self.0,
            extractor: |row| CoinOrderBorrowed {
                id: row.get(0),
                user_id: row.get(1),
                created_at: row.get(2),
                captured_at: row.get(3),
                coins: row.get(4),
                invoice_number: row.get(5),
            },
            mapper: |it| CoinOrder::from(it),
        }
    }
}
pub fn get_coin_order() -> GetCoinOrderStmt {
    GetCoinOrderStmt(crate::client::async_::Stmt::new(
        "select * from paypal_coin_orders where id=$1",
    ))
}
pub struct GetCoinOrderStmt(crate::client::async_::Stmt);
impl GetCoinOrderStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s mut self,
        client: &'c C,
        id: &'a T1,
    ) -> CoinOrderQuery<'c, 'a, 's, C, CoinOrder, 1> {
        CoinOrderQuery {
            client,
            params: [id],
            stmt: &mut self.0,
            extractor: |row| CoinOrderBorrowed {
                id: row.get(0),
                user_id: row.get(1),
                created_at: row.get(2),
                captured_at: row.get(3),
                coins: row.get(4),
                invoice_number: row.get(5),
            },
            mapper: |it| CoinOrder::from(it),
        }
    }
}
pub fn get_coin_order_by_invoice_number() -> GetCoinOrderByInvoiceNumberStmt {
    GetCoinOrderByInvoiceNumberStmt(crate::client::async_::Stmt::new(
        "select * from paypal_coin_orders where invoice_number=$1",
    ))
}
pub struct GetCoinOrderByInvoiceNumberStmt(crate::client::async_::Stmt);
impl GetCoinOrderByInvoiceNumberStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        invoice_number: &'a i64,
    ) -> CoinOrderQuery<'c, 'a, 's, C, CoinOrder, 1> {
        CoinOrderQuery {
            client,
            params: [invoice_number],
            stmt: &mut self.0,
            extractor: |row| CoinOrderBorrowed {
                id: row.get(0),
                user_id: row.get(1),
                created_at: row.get(2),
                captured_at: row.get(3),
                coins: row.get(4),
                invoice_number: row.get(5),
            },
            mapper: |it| CoinOrder::from(it),
        }
    }
}
pub fn capture_coin_order() -> CaptureCoinOrderStmt {
    CaptureCoinOrderStmt(crate::client::async_::Stmt::new(
        "update paypal_coin_orders set captured_at=$1 where id=$2",
    ))
}
pub struct CaptureCoinOrderStmt(crate::client::async_::Stmt);
impl CaptureCoinOrderStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s mut self,
        client: &'c C,
        captured_at: &'a crate::types::time::TimestampTz,
        id: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[captured_at, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        CaptureCoinOrderParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CaptureCoinOrderStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a CaptureCoinOrderParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.captured_at, &params.id))
    }
}
pub fn get_next_invoice_number() -> GetNextInvoiceNumberStmt {
    GetNextInvoiceNumberStmt(crate::client::async_::Stmt::new(
        "select nextval('invoice_number')",
    ))
}
pub struct GetNextInvoiceNumberStmt(crate::client::async_::Stmt);
impl GetNextInvoiceNumberStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
    ) -> I64Query<'c, 'a, 's, C, i64, 0> {
        I64Query {
            client,
            params: [],
            stmt: &mut self.0,
            extractor: |row| row.get(0),
            mapper: |it| it,
        }
    }
}
