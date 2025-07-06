// This file was generated with `clorinde`. Do not modify.

#[derive(Clone, Copy, Debug)]
pub struct AddCoinsParams {
    pub user_id: uuid::Uuid,
    pub coins: i64,
    pub withheld_coins: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct ListTransactionsParams {
    pub user_id: uuid::Uuid,
    pub start: chrono::DateTime<chrono::FixedOffset>,
    pub end: chrono::DateTime<chrono::FixedOffset>,
}
#[derive(Debug)]
pub struct CreateTransactionParams<T1: crate::StringSql> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub coins: i64,
    pub description: Option<T1>,
    pub include_in_credit_note: bool,
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct Balance {
    pub coins: i64,
    pub withheld_coins: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub coins: i64,
    pub description: Option<String>,
    pub include_in_credit_note: bool,
}
pub struct TransactionBorrowed<'a> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub coins: i64,
    pub description: Option<&'a str>,
    pub include_in_credit_note: bool,
}
impl<'a> From<TransactionBorrowed<'a>> for Transaction {
    fn from(
        TransactionBorrowed {
            id,
            user_id,
            created_at,
            coins,
            description,
            include_in_credit_note,
        }: TransactionBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            user_id,
            created_at,
            coins,
            description: description.map(|v| v.into()),
            include_in_credit_note,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct BalanceQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<Balance, tokio_postgres::Error>,
    mapper: fn(Balance) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BalanceQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(Balance) -> R) -> BalanceQuery<'c, 'a, 's, C, R, N> {
        BalanceQuery {
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
pub struct TransactionQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<TransactionBorrowed, tokio_postgres::Error>,
    mapper: fn(TransactionBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> TransactionQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(TransactionBorrowed) -> R,
    ) -> TransactionQuery<'c, 'a, 's, C, R, N> {
        TransactionQuery {
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
pub struct GetBalanceStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_balance() -> GetBalanceStmt {
    GetBalanceStmt(
        "select coins, withheld_coins from coins where user_id=$1",
        None,
    )
}
impl GetBalanceStmt {
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
    ) -> BalanceQuery<'c, 'a, 's, C, Balance, 1> {
        BalanceQuery {
            client,
            params: [user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &tokio_postgres::Row| -> Result<Balance, tokio_postgres::Error> {
                Ok(Balance {
                    coins: row.try_get(0)?,
                    withheld_coins: row.try_get(1)?,
                })
            },
            mapper: |it| Balance::from(it),
        }
    }
}
pub struct AddCoinsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn add_coins() -> AddCoinsStmt {
    AddCoinsStmt(
        "merge into coins using (select $1::uuid as user_id) as u on coins.user_id=u.user_id when not matched then insert (user_id, coins, withheld_coins) values ($1, $2, $3) when matched then update set coins=coins+$2, withheld_coins=withheld_coins+$3 returning coins, withheld_coins",
        None,
    )
}
impl AddCoinsStmt {
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
        coins: &'a i64,
        withheld_coins: &'a i64,
    ) -> BalanceQuery<'c, 'a, 's, C, Balance, 3> {
        BalanceQuery {
            client,
            params: [user_id, coins, withheld_coins],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &tokio_postgres::Row| -> Result<Balance, tokio_postgres::Error> {
                Ok(Balance {
                    coins: row.try_get(0)?,
                    withheld_coins: row.try_get(1)?,
                })
            },
            mapper: |it| Balance::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        AddCoinsParams,
        BalanceQuery<'c, 'a, 's, C, Balance, 3>,
        C,
    > for AddCoinsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a AddCoinsParams,
    ) -> BalanceQuery<'c, 'a, 's, C, Balance, 3> {
        self.bind(
            client,
            &params.user_id,
            &params.coins,
            &params.withheld_coins,
        )
    }
}
pub struct ReleaseCoinsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn release_coins() -> ReleaseCoinsStmt {
    ReleaseCoinsStmt(
        "update coins set coins=coins+withheld_coins, withheld_coins=0 where user_id=$1",
        None,
    )
}
impl ReleaseCoinsStmt {
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
        user_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[user_id]).await
    }
}
pub struct ListTransactionsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_transactions() -> ListTransactionsStmt {
    ListTransactionsStmt(
        "select * from transactions where user_id=$1 and $2 <= created_at and created_at < $3 order by created_at asc",
        None,
    )
}
impl ListTransactionsStmt {
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
        start: &'a chrono::DateTime<chrono::FixedOffset>,
        end: &'a chrono::DateTime<chrono::FixedOffset>,
    ) -> TransactionQuery<'c, 'a, 's, C, Transaction, 3> {
        TransactionQuery {
            client,
            params: [user_id, start, end],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<TransactionBorrowed, tokio_postgres::Error> {
                    Ok(TransactionBorrowed {
                        id: row.try_get(0)?,
                        user_id: row.try_get(1)?,
                        created_at: row.try_get(2)?,
                        coins: row.try_get(3)?,
                        description: row.try_get(4)?,
                        include_in_credit_note: row.try_get(5)?,
                    })
                },
            mapper: |it| Transaction::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListTransactionsParams,
        TransactionQuery<'c, 'a, 's, C, Transaction, 3>,
        C,
    > for ListTransactionsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListTransactionsParams,
    ) -> TransactionQuery<'c, 'a, 's, C, Transaction, 3> {
        self.bind(client, &params.user_id, &params.start, &params.end)
    }
}
pub struct CreateTransactionStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn create_transaction() -> CreateTransactionStmt {
    CreateTransactionStmt(
        "insert into transactions (id, user_id, created_at, coins, description, include_in_credit_note) values ($1, $2, $3, $4, $5, $6)",
        None,
    )
}
impl CreateTransactionStmt {
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
        id: &'a uuid::Uuid,
        user_id: &'a uuid::Uuid,
        created_at: &'a chrono::DateTime<chrono::FixedOffset>,
        coins: &'a i64,
        description: &'a Option<T1>,
        include_in_credit_note: &'a bool,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    id,
                    user_id,
                    created_at,
                    coins,
                    description,
                    include_in_credit_note,
                ],
            )
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        CreateTransactionParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CreateTransactionStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a CreateTransactionParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.id,
            &params.user_id,
            &params.created_at,
            &params.coins,
            &params.description,
            &params.include_in_credit_note,
        ))
    }
}
