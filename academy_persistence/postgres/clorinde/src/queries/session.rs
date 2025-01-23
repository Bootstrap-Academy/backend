// This file was generated with `clorinde`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub device_name: Option<T1>,
    pub created_at: crate::types::time::TimestampTz,
    pub updated_at: crate::types::time::TimestampTz,
}
#[derive(Debug)]
pub struct UpdateParams<T1: crate::StringSql> {
    pub clear_device_name: bool,
    pub device_name: Option<T1>,
    pub updated_at: Option<crate::types::time::TimestampTz>,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct SetRefreshTokenHashParams<T1: crate::BytesSql> {
    pub session_id: uuid::Uuid,
    pub refresh_token_hash: T1,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub device_name: Option<String>,
    pub created_at: crate::types::time::TimestampTz,
    pub updated_at: crate::types::time::TimestampTz,
}
pub struct SessionBorrowed<'a> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub device_name: Option<&'a str>,
    pub created_at: crate::types::time::TimestampTz,
    pub updated_at: crate::types::time::TimestampTz,
}
impl<'a> From<SessionBorrowed<'a>> for Session {
    fn from(
        SessionBorrowed {
            id,
            user_id,
            device_name,
            created_at,
            updated_at,
        }: SessionBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            user_id,
            device_name: device_name.map(|v| v.into()),
            created_at,
            updated_at,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct SessionQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> SessionBorrowed,
    mapper: fn(SessionBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> SessionQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(SessionBorrowed) -> R) -> SessionQuery<'c, 'a, 's, C, R, N> {
        SessionQuery {
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
pub struct Vecu8Query<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> &[u8],
    mapper: fn(&[u8]) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> Vecu8Query<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(&[u8]) -> R) -> Vecu8Query<'c, 'a, 's, C, R, N> {
        Vecu8Query {
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
pub fn get() -> GetStmt {
    GetStmt(crate::client::async_::Stmt::new(
        "select * from sessions where id=$1",
    ))
}
pub struct GetStmt(crate::client::async_::Stmt);
impl GetStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        id: &'a uuid::Uuid,
    ) -> SessionQuery<'c, 'a, 's, C, Session, 1> {
        SessionQuery {
            client,
            params: [id],
            stmt: &mut self.0,
            extractor: |row| SessionBorrowed {
                id: row.get(0),
                user_id: row.get(1),
                device_name: row.get(2),
                created_at: row.get(3),
                updated_at: row.get(4),
            },
            mapper: |it| <Session>::from(it),
        }
    }
}
pub fn get_by_refresh_token_hash() -> GetByRefreshTokenHashStmt {
    GetByRefreshTokenHashStmt(crate::client::async_::Stmt::new(
        "select s.* from sessions s
  inner join session_refresh_tokens rt
  on s.id=rt.session_id
  where rt.refresh_token_hash=$1",
    ))
}
pub struct GetByRefreshTokenHashStmt(crate::client::async_::Stmt);
impl GetByRefreshTokenHashStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql>(
        &'s mut self,
        client: &'c C,
        refresh_token_hash: &'a T1,
    ) -> SessionQuery<'c, 'a, 's, C, Session, 1> {
        SessionQuery {
            client,
            params: [refresh_token_hash],
            stmt: &mut self.0,
            extractor: |row| SessionBorrowed {
                id: row.get(0),
                user_id: row.get(1),
                device_name: row.get(2),
                created_at: row.get(3),
                updated_at: row.get(4),
            },
            mapper: |it| <Session>::from(it),
        }
    }
}
pub fn list_by_user() -> ListByUserStmt {
    ListByUserStmt(crate::client::async_::Stmt::new(
        "select * from sessions where user_id=$1",
    ))
}
pub struct ListByUserStmt(crate::client::async_::Stmt);
impl ListByUserStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> SessionQuery<'c, 'a, 's, C, Session, 1> {
        SessionQuery {
            client,
            params: [user_id],
            stmt: &mut self.0,
            extractor: |row| SessionBorrowed {
                id: row.get(0),
                user_id: row.get(1),
                device_name: row.get(2),
                created_at: row.get(3),
                updated_at: row.get(4),
            },
            mapper: |it| <Session>::from(it),
        }
    }
}
pub fn create() -> CreateStmt {
    CreateStmt(crate::client::async_::Stmt::new(
        "insert into sessions (id, user_id, device_name, created_at, updated_at)
  values ($1, $2, $3, $4, $5)",
    ))
}
pub struct CreateStmt(crate::client::async_::Stmt);
impl CreateStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s mut self,
        client: &'c C,
        id: &'a uuid::Uuid,
        user_id: &'a uuid::Uuid,
        device_name: &'a Option<T1>,
        created_at: &'a crate::types::time::TimestampTz,
        updated_at: &'a crate::types::time::TimestampTz,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client
            .execute(stmt, &[id, user_id, device_name, created_at, updated_at])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        CreateParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CreateStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a CreateParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.id,
            &params.user_id,
            &params.device_name,
            &params.created_at,
            &params.updated_at,
        ))
    }
}
pub fn update() -> UpdateStmt {
    UpdateStmt(crate::client::async_::Stmt::new(
        "update sessions
  set
    device_name=case when $1 then null else coalesce($2, device_name) end,
    updated_at=coalesce($3, updated_at)
  where id=$4",
    ))
}
pub struct UpdateStmt(crate::client::async_::Stmt);
impl UpdateStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s mut self,
        client: &'c C,
        clear_device_name: &'a bool,
        device_name: &'a Option<T1>,
        updated_at: &'a Option<crate::types::time::TimestampTz>,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client
            .execute(stmt, &[clear_device_name, device_name, updated_at, id])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpdateParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpdateStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a UpdateParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.clear_device_name,
            &params.device_name,
            &params.updated_at,
            &params.id,
        ))
    }
}
pub fn delete() -> DeleteStmt {
    DeleteStmt(crate::client::async_::Stmt::new(
        "delete from sessions where id=$1",
    ))
}
pub struct DeleteStmt(crate::client::async_::Stmt);
impl DeleteStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[id]).await
    }
}
pub fn delete_by_user() -> DeleteByUserStmt {
    DeleteByUserStmt(crate::client::async_::Stmt::new(
        "delete from sessions where user_id=$1",
    ))
}
pub struct DeleteByUserStmt(crate::client::async_::Stmt);
impl DeleteByUserStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[user_id]).await
    }
}
pub fn delete_by_updated_at() -> DeleteByUpdatedAtStmt {
    DeleteByUpdatedAtStmt(crate::client::async_::Stmt::new(
        "delete from sessions where updated_at<$1",
    ))
}
pub struct DeleteByUpdatedAtStmt(crate::client::async_::Stmt);
impl DeleteByUpdatedAtStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        updated_at: &'a crate::types::time::TimestampTz,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[updated_at]).await
    }
}
pub fn list_refresh_token_hashes_by_user() -> ListRefreshTokenHashesByUserStmt {
    ListRefreshTokenHashesByUserStmt(crate::client::async_::Stmt::new(
        "select rt.refresh_token_hash
  from session_refresh_tokens rt
  inner join sessions s on s.id=rt.session_id
  where s.user_id=$1",
    ))
}
pub struct ListRefreshTokenHashesByUserStmt(crate::client::async_::Stmt);
impl ListRefreshTokenHashesByUserStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> Vecu8Query<'c, 'a, 's, C, Vec<u8>, 1> {
        Vecu8Query {
            client,
            params: [user_id],
            stmt: &mut self.0,
            extractor: |row| row.get(0),
            mapper: |it| it.into(),
        }
    }
}
pub fn get_refresh_token_hash() -> GetRefreshTokenHashStmt {
    GetRefreshTokenHashStmt(crate::client::async_::Stmt::new(
        "select refresh_token_hash from session_refresh_tokens where session_id=$1",
    ))
}
pub struct GetRefreshTokenHashStmt(crate::client::async_::Stmt);
impl GetRefreshTokenHashStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        session_id: &'a uuid::Uuid,
    ) -> Vecu8Query<'c, 'a, 's, C, Vec<u8>, 1> {
        Vecu8Query {
            client,
            params: [session_id],
            stmt: &mut self.0,
            extractor: |row| row.get(0),
            mapper: |it| it.into(),
        }
    }
}
pub fn set_refresh_token_hash() -> SetRefreshTokenHashStmt {
    SetRefreshTokenHashStmt(crate::client::async_::Stmt::new(
        "insert into session_refresh_tokens (session_id, refresh_token_hash)
  values ($1, $2)
  on conflict (session_id) do update set refresh_token_hash=$2",
    ))
}
pub struct SetRefreshTokenHashStmt(crate::client::async_::Stmt);
impl SetRefreshTokenHashStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql>(
        &'s mut self,
        client: &'c C,
        session_id: &'a uuid::Uuid,
        refresh_token_hash: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client
            .execute(stmt, &[session_id, refresh_token_hash])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::BytesSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SetRefreshTokenHashParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SetRefreshTokenHashStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a SetRefreshTokenHashParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.session_id, &params.refresh_token_hash))
    }
}
