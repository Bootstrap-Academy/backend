// This file was generated with `clorinde`. Do not modify.

#[derive(Clone, Copy, Debug)]
pub struct CreateParams {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub since: crate::types::time::TimestampTz,
    pub until: crate::types::time::TimestampTz,
}
#[derive(Clone, Copy, Debug)]
pub struct ExtendParams {
    pub until: crate::types::time::TimestampTz,
    pub id: uuid::Uuid,
}
#[derive(Clone, Copy, Debug)]
pub struct SetSubscriptionParams {
    pub user_id: uuid::Uuid,
    pub plan: Option<crate::types::PremiumPlan>,
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct Premium {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub since: crate::types::time::TimestampTz,
    pub until: crate::types::time::TimestampTz,
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct PremiumQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> Premium,
    mapper: fn(Premium) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> PremiumQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(Premium) -> R) -> PremiumQuery<'c, 'a, 's, C, R, N> {
        PremiumQuery {
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
pub struct UuidUuidQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> uuid::Uuid,
    mapper: fn(uuid::Uuid) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> UuidUuidQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(uuid::Uuid) -> R) -> UuidUuidQuery<'c, 'a, 's, C, R, N> {
        UuidUuidQuery {
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
pub struct PremiumPlanQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> crate::types::PremiumPlan,
    mapper: fn(crate::types::PremiumPlan) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> PremiumPlanQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(crate::types::PremiumPlan) -> R,
    ) -> PremiumPlanQuery<'c, 'a, 's, C, R, N> {
        PremiumPlanQuery {
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
pub fn get_latest_by_user_id() -> GetLatestByUserIdStmt {
    GetLatestByUserIdStmt(crate::client::async_::Stmt::new(
        "select * from premium where user_id=$1 order by until desc limit 1",
    ))
}
pub struct GetLatestByUserIdStmt(crate::client::async_::Stmt);
impl GetLatestByUserIdStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> PremiumQuery<'c, 'a, 's, C, Premium, 1> {
        PremiumQuery {
            client,
            params: [user_id],
            stmt: &mut self.0,
            extractor: |row| Premium {
                id: row.get(0),
                user_id: row.get(1),
                since: row.get(2),
                until: row.get(3),
            },
            mapper: |it| <Premium>::from(it),
        }
    }
}
pub fn create() -> CreateStmt {
    CreateStmt(crate::client::async_::Stmt::new(
        "insert into premium (id, user_id, since, until)
  values ($1, $2, $3, $4)",
    ))
}
pub struct CreateStmt(crate::client::async_::Stmt);
impl CreateStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        id: &'a uuid::Uuid,
        user_id: &'a uuid::Uuid,
        since: &'a crate::types::time::TimestampTz,
        until: &'a crate::types::time::TimestampTz,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[id, user_id, since, until]).await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        CreateParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CreateStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a CreateParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.id,
            &params.user_id,
            &params.since,
            &params.until,
        ))
    }
}
pub fn extend() -> ExtendStmt {
    ExtendStmt(crate::client::async_::Stmt::new(
        "update premium set until=$1 where id=$2",
    ))
}
pub struct ExtendStmt(crate::client::async_::Stmt);
impl ExtendStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        until: &'a crate::types::time::TimestampTz,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[until, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        ExtendParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for ExtendStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a ExtendParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.until, &params.id))
    }
}
pub fn list_subscription_users() -> ListSubscriptionUsersStmt {
    ListSubscriptionUsersStmt(crate::client::async_::Stmt::new(
        "select user_id from premium_subscriptions",
    ))
}
pub struct ListSubscriptionUsersStmt(crate::client::async_::Stmt);
impl ListSubscriptionUsersStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
    ) -> UuidUuidQuery<'c, 'a, 's, C, uuid::Uuid, 0> {
        UuidUuidQuery {
            client,
            params: [],
            stmt: &mut self.0,
            extractor: |row| row.get(0),
            mapper: |it| it,
        }
    }
}
pub fn get_subscription() -> GetSubscriptionStmt {
    GetSubscriptionStmt(crate::client::async_::Stmt::new(
        "select plan from premium_subscriptions where user_id=$1",
    ))
}
pub struct GetSubscriptionStmt(crate::client::async_::Stmt);
impl GetSubscriptionStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> PremiumPlanQuery<'c, 'a, 's, C, crate::types::PremiumPlan, 1> {
        PremiumPlanQuery {
            client,
            params: [user_id],
            stmt: &mut self.0,
            extractor: |row| row.get(0),
            mapper: |it| it,
        }
    }
}
pub fn set_subscription() -> SetSubscriptionStmt {
    SetSubscriptionStmt(crate::client::async_::Stmt::new(
        "merge into premium_subscriptions
  using (select $1::uuid as user_id where $2::premium_plan is not null) as s
  on premium_subscriptions.user_id = s.user_id
  when not matched by target then insert (user_id, plan) values ($1, $2)
  when not matched by source then delete
  when matched then update set plan=$2",
    ))
}
pub struct SetSubscriptionStmt(crate::client::async_::Stmt);
impl SetSubscriptionStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
        plan: &'a Option<crate::types::PremiumPlan>,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[user_id, plan]).await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SetSubscriptionParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SetSubscriptionStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a SetSubscriptionParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.user_id, &params.plan))
    }
}
