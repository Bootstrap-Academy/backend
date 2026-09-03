// This file was generated with `clorinde`. Do not modify.

#[derive(Debug)]
pub struct CreateCoinOrderParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub id: T1,
    pub user_id: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub captured_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub coins: i64,
    pub invoice_number: i64,
    pub withdrawal_consent_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub withdrawal_text_version: Option<T2>,
}
#[derive(Debug)]
pub struct CaptureCoinOrderParams<T1: crate::StringSql> {
    pub captured_at: chrono::DateTime<chrono::FixedOffset>,
    pub id: T1,
}
#[derive(Debug, Clone, PartialEq)]
pub struct CoinOrder {
    pub id: String,
    pub user_id: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub captured_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub coins: i64,
    pub invoice_number: i64,
    pub withdrawal_consent_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub withdrawal_text_version: Option<String>,
}
pub struct CoinOrderBorrowed<'a> {
    pub id: &'a str,
    pub user_id: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub captured_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub coins: i64,
    pub invoice_number: i64,
    pub withdrawal_consent_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub withdrawal_text_version: Option<&'a str>,
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
            withdrawal_consent_at,
            withdrawal_text_version,
        }: CoinOrderBorrowed<'a>,
    ) -> Self {
        Self {
            id: id.into(),
            user_id,
            created_at,
            captured_at,
            coins,
            invoice_number,
            withdrawal_consent_at,
            withdrawal_text_version: withdrawal_text_version.map(|v| v.into()),
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
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
pub struct CoinOrderQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<CoinOrderBorrowed, tokio_postgres::Error>,
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
pub struct CreateCoinOrderStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn create_coin_order() -> CreateCoinOrderStmt {
    CreateCoinOrderStmt(
        "insert into paypal_coin_orders (id, user_id, created_at, captured_at, coins, invoice_number, withdrawal_consent_at, withdrawal_text_version) values ($1, $2, $3, $4, $5, $6, $7, $8)",
        None,
    )
}
impl CreateCoinOrderStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        id: &'a T1,
        user_id: &'a uuid::Uuid,
        created_at: &'a chrono::DateTime<chrono::FixedOffset>,
        captured_at: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
        coins: &'a i64,
        invoice_number: &'a i64,
        withdrawal_consent_at: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
        withdrawal_text_version: &'a Option<T2>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    id,
                    user_id,
                    created_at,
                    captured_at,
                    coins,
                    invoice_number,
                    withdrawal_consent_at,
                    withdrawal_text_version,
                ],
            )
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        CreateCoinOrderParams<T1, T2>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CreateCoinOrderStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a CreateCoinOrderParams<T1, T2>,
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
            &params.withdrawal_consent_at,
            &params.withdrawal_text_version,
        ))
    }
}
pub struct CountCoinOrdersStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_coin_orders() -> CountCoinOrdersStmt {
    CountCoinOrdersStmt("select count(*) from paypal_coin_orders", None)
}
impl CountCoinOrdersStmt {
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
    ) -> I64Query<'c, 'a, 's, C, i64, 0> {
        I64Query {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct ListCoinOrdersStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_coin_orders() -> ListCoinOrdersStmt {
    ListCoinOrdersStmt("select * from paypal_coin_orders", None)
}
impl ListCoinOrdersStmt {
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
    ) -> CoinOrderQuery<'c, 'a, 's, C, CoinOrder, 0> {
        CoinOrderQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<CoinOrderBorrowed, tokio_postgres::Error> {
                    Ok(CoinOrderBorrowed {
                        id: row.try_get(0)?,
                        user_id: row.try_get(1)?,
                        created_at: row.try_get(2)?,
                        captured_at: row.try_get(3)?,
                        coins: row.try_get(4)?,
                        invoice_number: row.try_get(5)?,
                        withdrawal_consent_at: row.try_get(6)?,
                        withdrawal_text_version: row.try_get(7)?,
                    })
                },
            mapper: |it| CoinOrder::from(it),
        }
    }
}
pub struct GetCoinOrderStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_coin_order() -> GetCoinOrderStmt {
    GetCoinOrderStmt("select * from paypal_coin_orders where id=$1", None)
}
impl GetCoinOrderStmt {
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
        id: &'a T1,
    ) -> CoinOrderQuery<'c, 'a, 's, C, CoinOrder, 1> {
        CoinOrderQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<CoinOrderBorrowed, tokio_postgres::Error> {
                    Ok(CoinOrderBorrowed {
                        id: row.try_get(0)?,
                        user_id: row.try_get(1)?,
                        created_at: row.try_get(2)?,
                        captured_at: row.try_get(3)?,
                        coins: row.try_get(4)?,
                        invoice_number: row.try_get(5)?,
                        withdrawal_consent_at: row.try_get(6)?,
                        withdrawal_text_version: row.try_get(7)?,
                    })
                },
            mapper: |it| CoinOrder::from(it),
        }
    }
}
pub struct GetCoinOrderByInvoiceNumberStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_coin_order_by_invoice_number() -> GetCoinOrderByInvoiceNumberStmt {
    GetCoinOrderByInvoiceNumberStmt(
        "select * from paypal_coin_orders where invoice_number=$1",
        None,
    )
}
impl GetCoinOrderByInvoiceNumberStmt {
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
        invoice_number: &'a i64,
    ) -> CoinOrderQuery<'c, 'a, 's, C, CoinOrder, 1> {
        CoinOrderQuery {
            client,
            params: [invoice_number],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<CoinOrderBorrowed, tokio_postgres::Error> {
                    Ok(CoinOrderBorrowed {
                        id: row.try_get(0)?,
                        user_id: row.try_get(1)?,
                        created_at: row.try_get(2)?,
                        captured_at: row.try_get(3)?,
                        coins: row.try_get(4)?,
                        invoice_number: row.try_get(5)?,
                        withdrawal_consent_at: row.try_get(6)?,
                        withdrawal_text_version: row.try_get(7)?,
                    })
                },
            mapper: |it| CoinOrder::from(it),
        }
    }
}
pub struct CaptureCoinOrderStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn capture_coin_order() -> CaptureCoinOrderStmt {
    CaptureCoinOrderStmt(
        "update paypal_coin_orders set captured_at=$1 where id=$2",
        None,
    )
}
impl CaptureCoinOrderStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        captured_at: &'a chrono::DateTime<chrono::FixedOffset>,
        id: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[captured_at, id]).await
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
        &'a self,
        client: &'a C,
        params: &'a CaptureCoinOrderParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.captured_at, &params.id))
    }
}
pub struct GetNextInvoiceNumberStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_next_invoice_number() -> GetNextInvoiceNumberStmt {
    GetNextInvoiceNumberStmt("select nextval('invoice_number')", None)
}
impl GetNextInvoiceNumberStmt {
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
    ) -> I64Query<'c, 'a, 's, C, i64, 0> {
        I64Query {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
