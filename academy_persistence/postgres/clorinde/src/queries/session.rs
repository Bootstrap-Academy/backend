// This file was generated with `clorinde`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub device_name: Option<T1>,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
    pub mfa_verified: bool,
}
#[derive(Debug)]
pub struct UpdateParams<T1: crate::StringSql> {
    pub clear_device_name: bool,
    pub device_name: Option<T1>,
    pub updated_at: Option<chrono::DateTime<chrono::FixedOffset>>,
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
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
    pub mfa_verified: bool,
}
pub struct SessionBorrowed<'a> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub device_name: Option<&'a str>,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
    pub mfa_verified: bool,
}
impl<'a> From<SessionBorrowed<'a>> for Session {
    fn from(
        SessionBorrowed {
            id,
            user_id,
            device_name,
            created_at,
            updated_at,
            mfa_verified,
        }: SessionBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            user_id,
            device_name: device_name.map(|v| v.into()),
            created_at,
            updated_at,
            mfa_verified,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct SessionQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<SessionBorrowed, tokio_postgres::Error>,
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
pub struct Vecu8Query<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<&[u8], tokio_postgres::Error>,
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
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("select * from sessions where id=$1", None)
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
    ) -> SessionQuery<'c, 'a, 's, C, Session, 1> {
        SessionQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<SessionBorrowed, tokio_postgres::Error> {
                    Ok(SessionBorrowed {
                        id: row.try_get(0)?,
                        user_id: row.try_get(1)?,
                        device_name: row.try_get(2)?,
                        created_at: row.try_get(3)?,
                        updated_at: row.try_get(4)?,
                        mfa_verified: row.try_get(5)?,
                    })
                },
            mapper: |it| Session::from(it),
        }
    }
}
pub struct GetByRefreshTokenHashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_by_refresh_token_hash() -> GetByRefreshTokenHashStmt {
    GetByRefreshTokenHashStmt(
        "select s.* from sessions s inner join session_refresh_tokens rt on s.id=rt.session_id where rt.refresh_token_hash=$1",
        None,
    )
}
impl GetByRefreshTokenHashStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql>(
        &'s self,
        client: &'c C,
        refresh_token_hash: &'a T1,
    ) -> SessionQuery<'c, 'a, 's, C, Session, 1> {
        SessionQuery {
            client,
            params: [refresh_token_hash],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<SessionBorrowed, tokio_postgres::Error> {
                    Ok(SessionBorrowed {
                        id: row.try_get(0)?,
                        user_id: row.try_get(1)?,
                        device_name: row.try_get(2)?,
                        created_at: row.try_get(3)?,
                        updated_at: row.try_get(4)?,
                        mfa_verified: row.try_get(5)?,
                    })
                },
            mapper: |it| Session::from(it),
        }
    }
}
pub struct ListByUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_by_user() -> ListByUserStmt {
    ListByUserStmt("select * from sessions where user_id=$1", None)
}
impl ListByUserStmt {
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
    ) -> SessionQuery<'c, 'a, 's, C, Session, 1> {
        SessionQuery {
            client,
            params: [user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<SessionBorrowed, tokio_postgres::Error> {
                    Ok(SessionBorrowed {
                        id: row.try_get(0)?,
                        user_id: row.try_get(1)?,
                        device_name: row.try_get(2)?,
                        created_at: row.try_get(3)?,
                        updated_at: row.try_get(4)?,
                        mfa_verified: row.try_get(5)?,
                    })
                },
            mapper: |it| Session::from(it),
        }
    }
}
pub struct CreateStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn create() -> CreateStmt {
    CreateStmt(
        "insert into sessions (id, user_id, device_name, created_at, updated_at, mfa_verified) values ($1, $2, $3, $4, $5, $6)",
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
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        id: &'a uuid::Uuid,
        user_id: &'a uuid::Uuid,
        device_name: &'a Option<T1>,
        created_at: &'a chrono::DateTime<chrono::FixedOffset>,
        updated_at: &'a chrono::DateTime<chrono::FixedOffset>,
        mfa_verified: &'a bool,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    id,
                    user_id,
                    device_name,
                    created_at,
                    updated_at,
                    mfa_verified,
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
        CreateParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CreateStmt
{
    fn params(
        &'a self,
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
            &params.mfa_verified,
        ))
    }
}
pub struct UpdateStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update() -> UpdateStmt {
    UpdateStmt(
        "update sessions set device_name=case when $1 then null else coalesce($2, device_name) end, updated_at=coalesce($3, updated_at) where id=$4",
        None,
    )
}
impl UpdateStmt {
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
        clear_device_name: &'a bool,
        device_name: &'a Option<T1>,
        updated_at: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[clear_device_name, device_name, updated_at, id])
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
        &'a self,
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
pub struct DeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete() -> DeleteStmt {
    DeleteStmt("delete from sessions where id=$1", None)
}
impl DeleteStmt {
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
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[id]).await
    }
}
pub struct DeleteByUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_by_user() -> DeleteByUserStmt {
    DeleteByUserStmt("delete from sessions where user_id=$1", None)
}
impl DeleteByUserStmt {
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
pub struct DeleteByUpdatedAtStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_by_updated_at() -> DeleteByUpdatedAtStmt {
    DeleteByUpdatedAtStmt("delete from sessions where updated_at<$1", None)
}
impl DeleteByUpdatedAtStmt {
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
        updated_at: &'a chrono::DateTime<chrono::FixedOffset>,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[updated_at]).await
    }
}
pub struct ListRefreshTokenHashesByUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_refresh_token_hashes_by_user() -> ListRefreshTokenHashesByUserStmt {
    ListRefreshTokenHashesByUserStmt(
        "select rt.refresh_token_hash from session_refresh_tokens rt inner join sessions s on s.id=rt.session_id where s.user_id=$1",
        None,
    )
}
impl ListRefreshTokenHashesByUserStmt {
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
    ) -> Vecu8Query<'c, 'a, 's, C, Vec<u8>, 1> {
        Vecu8Query {
            client,
            params: [user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
pub struct GetRefreshTokenHashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_refresh_token_hash() -> GetRefreshTokenHashStmt {
    GetRefreshTokenHashStmt(
        "select refresh_token_hash from session_refresh_tokens where session_id=$1",
        None,
    )
}
impl GetRefreshTokenHashStmt {
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
        session_id: &'a uuid::Uuid,
    ) -> Vecu8Query<'c, 'a, 's, C, Vec<u8>, 1> {
        Vecu8Query {
            client,
            params: [session_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
pub struct SetRefreshTokenHashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_refresh_token_hash() -> SetRefreshTokenHashStmt {
    SetRefreshTokenHashStmt(
        "insert into session_refresh_tokens (session_id, refresh_token_hash) values ($1, $2) on conflict (session_id) do update set refresh_token_hash=$2",
        None,
    )
}
impl SetRefreshTokenHashStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql>(
        &'s self,
        client: &'c C,
        session_id: &'a uuid::Uuid,
        refresh_token_hash: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[session_id, refresh_token_hash])
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
        &'a self,
        client: &'a C,
        params: &'a SetRefreshTokenHashParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.session_id, &params.refresh_token_hash))
    }
}
