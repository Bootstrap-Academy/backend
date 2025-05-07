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
    pub start: crate::types::time::TimestampTz,
    pub end: crate::types::time::TimestampTz,
}
#[derive(Debug)]
pub struct CreateTransactionParams<T1: crate::StringSql> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub created_at: crate::types::time::TimestampTz,
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
    pub created_at: crate::types::time::TimestampTz,
    pub coins: i64,
    pub description: Option<String>,
    pub include_in_credit_note: bool,
}
pub struct TransactionBorrowed<'a> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub created_at: crate::types::time::TimestampTz,
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
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> Balance,
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
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + use<'c, C, T, N>,
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
pub struct TransactionQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> TransactionBorrowed,
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
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + use<'c, C, T, N>,
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
pub fn get_balance() -> GetBalanceStmt {
    GetBalanceStmt(crate::client::async_::Stmt::new(
        "select coins, withheld_coins from coins where user_id=$1",
    ))
}
pub struct GetBalanceStmt(crate::client::async_::Stmt);
impl GetBalanceStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> BalanceQuery<'c, 'a, 's, C, Balance, 1> {
        BalanceQuery {
            client,
            params: [user_id],
            stmt: &mut self.0,
            extractor: |row| Balance {
                coins: row.get(0),
                withheld_coins: row.get(1),
            },
            mapper: |it| Balance::from(it),
        }
    }
}
pub fn add_coins() -> AddCoinsStmt {
    AddCoinsStmt(crate::client::async_::Stmt::new(
        "merge into coins using (select $1::uuid as user_id) as u on coins.user_id=u.user_id when not matched then insert (user_id, coins, withheld_coins) values ($1, $2, $3) when matched then update set coins=coins+$2, withheld_coins=withheld_coins+$3 returning coins, withheld_coins",
    ))
}
pub struct AddCoinsStmt(crate::client::async_::Stmt);
impl AddCoinsStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
        coins: &'a i64,
        withheld_coins: &'a i64,
    ) -> BalanceQuery<'c, 'a, 's, C, Balance, 3> {
        BalanceQuery {
            client,
            params: [user_id, coins, withheld_coins],
            stmt: &mut self.0,
            extractor: |row| Balance {
                coins: row.get(0),
                withheld_coins: row.get(1),
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
        &'s mut self,
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
pub fn release_coins() -> ReleaseCoinsStmt {
    ReleaseCoinsStmt(crate::client::async_::Stmt::new(
        "update coins set coins=coins+withheld_coins, withheld_coins=0 where user_id=$1",
    ))
}
pub struct ReleaseCoinsStmt(crate::client::async_::Stmt);
impl ReleaseCoinsStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[user_id]).await
    }
}
pub fn list_transactions() -> ListTransactionsStmt {
    ListTransactionsStmt(crate::client::async_::Stmt::new(
        "select * from transactions where user_id=$1 and $2 <= created_at and created_at < $3 order by created_at asc",
    ))
}
pub struct ListTransactionsStmt(crate::client::async_::Stmt);
impl ListTransactionsStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
        start: &'a crate::types::time::TimestampTz,
        end: &'a crate::types::time::TimestampTz,
    ) -> TransactionQuery<'c, 'a, 's, C, Transaction, 3> {
        TransactionQuery {
            client,
            params: [user_id, start, end],
            stmt: &mut self.0,
            extractor: |row| TransactionBorrowed {
                id: row.get(0),
                user_id: row.get(1),
                created_at: row.get(2),
                coins: row.get(3),
                description: row.get(4),
                include_in_credit_note: row.get(5),
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
        &'s mut self,
        client: &'c C,
        params: &'a ListTransactionsParams,
    ) -> TransactionQuery<'c, 'a, 's, C, Transaction, 3> {
        self.bind(client, &params.user_id, &params.start, &params.end)
    }
}
pub fn create_transaction() -> CreateTransactionStmt {
    CreateTransactionStmt(crate::client::async_::Stmt::new(
        "insert into transactions (id, user_id, created_at, coins, description, include_in_credit_note) values ($1, $2, $3, $4, $5, $6)",
    ))
}
pub struct CreateTransactionStmt(crate::client::async_::Stmt);
impl CreateTransactionStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s mut self,
        client: &'c C,
        id: &'a uuid::Uuid,
        user_id: &'a uuid::Uuid,
        created_at: &'a crate::types::time::TimestampTz,
        coins: &'a i64,
        description: &'a Option<T1>,
        include_in_credit_note: &'a bool,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client
            .execute(
                stmt,
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
        &'a mut self,
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
