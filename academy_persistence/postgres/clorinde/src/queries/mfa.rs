// This file was generated with `clorinde`. Do not modify.

#[derive(Clone, Copy, Debug)]
pub struct CreateTotpDeviceParams {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub enabled: bool,
    pub created_at: crate::types::time::TimestampTz,
}
#[derive(Clone, Copy, Debug)]
pub struct UpdateTotpDeviceParams {
    pub enabled: Option<bool>,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct SetTotpDeviceSecretParams<T1: crate::BytesSql> {
    pub id: uuid::Uuid,
    pub secret: T1,
}
#[derive(Debug)]
pub struct SetRecoveryCodeHashParams<T1: crate::BytesSql> {
    pub user_id: uuid::Uuid,
    pub code: T1,
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct TotpDevice {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub enabled: bool,
    pub created_at: crate::types::time::TimestampTz,
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct TotpDeviceQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> TotpDevice,
    mapper: fn(TotpDevice) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> TotpDeviceQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(TotpDevice) -> R) -> TotpDeviceQuery<'c, 'a, 's, C, R, N> {
        TotpDeviceQuery {
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
pub fn list_totp_devices_by_user() -> ListTotpDevicesByUserStmt {
    ListTotpDevicesByUserStmt(crate::client::async_::Stmt::new(
        "select * from totp_devices where user_id=$1",
    ))
}
pub struct ListTotpDevicesByUserStmt(crate::client::async_::Stmt);
impl ListTotpDevicesByUserStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> TotpDeviceQuery<'c, 'a, 's, C, TotpDevice, 1> {
        TotpDeviceQuery {
            client,
            params: [user_id],
            stmt: &mut self.0,
            extractor: |row| TotpDevice {
                id: row.get(0),
                user_id: row.get(1),
                enabled: row.get(2),
                created_at: row.get(3),
            },
            mapper: |it| TotpDevice::from(it),
        }
    }
}
pub fn create_totp_device() -> CreateTotpDeviceStmt {
    CreateTotpDeviceStmt(crate::client::async_::Stmt::new(
        "insert into totp_devices (id, user_id, enabled, created_at) values ($1, $2, $3, $4)",
    ))
}
pub struct CreateTotpDeviceStmt(crate::client::async_::Stmt);
impl CreateTotpDeviceStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        id: &'a uuid::Uuid,
        user_id: &'a uuid::Uuid,
        enabled: &'a bool,
        created_at: &'a crate::types::time::TimestampTz,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client
            .execute(stmt, &[id, user_id, enabled, created_at])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        CreateTotpDeviceParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CreateTotpDeviceStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a CreateTotpDeviceParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.id,
            &params.user_id,
            &params.enabled,
            &params.created_at,
        ))
    }
}
pub fn update_totp_device() -> UpdateTotpDeviceStmt {
    UpdateTotpDeviceStmt(crate::client::async_::Stmt::new(
        "update totp_devices set enabled=coalesce($1, enabled) where id=$2",
    ))
}
pub struct UpdateTotpDeviceStmt(crate::client::async_::Stmt);
impl UpdateTotpDeviceStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        enabled: &'a Option<bool>,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[enabled, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpdateTotpDeviceParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpdateTotpDeviceStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a UpdateTotpDeviceParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.enabled, &params.id))
    }
}
pub fn delete_totp_devices_by_user() -> DeleteTotpDevicesByUserStmt {
    DeleteTotpDevicesByUserStmt(crate::client::async_::Stmt::new(
        "delete from totp_devices where user_id=$1",
    ))
}
pub struct DeleteTotpDevicesByUserStmt(crate::client::async_::Stmt);
impl DeleteTotpDevicesByUserStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[user_id]).await
    }
}
pub fn list_enabled_totp_device_secrets_by_user() -> ListEnabledTotpDeviceSecretsByUserStmt {
    ListEnabledTotpDeviceSecretsByUserStmt(crate::client::async_::Stmt::new(
        "select secret from totp_device_secrets inner join totp_devices using(id) where user_id=$1 and enabled",
    ))
}
pub struct ListEnabledTotpDeviceSecretsByUserStmt(crate::client::async_::Stmt);
impl ListEnabledTotpDeviceSecretsByUserStmt {
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
pub fn get_totp_device_secret() -> GetTotpDeviceSecretStmt {
    GetTotpDeviceSecretStmt(crate::client::async_::Stmt::new(
        "select secret from totp_device_secrets where id=$1",
    ))
}
pub struct GetTotpDeviceSecretStmt(crate::client::async_::Stmt);
impl GetTotpDeviceSecretStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        id: &'a uuid::Uuid,
    ) -> Vecu8Query<'c, 'a, 's, C, Vec<u8>, 1> {
        Vecu8Query {
            client,
            params: [id],
            stmt: &mut self.0,
            extractor: |row| row.get(0),
            mapper: |it| it.into(),
        }
    }
}
pub fn set_totp_device_secret() -> SetTotpDeviceSecretStmt {
    SetTotpDeviceSecretStmt(crate::client::async_::Stmt::new(
        "insert into totp_device_secrets (id, secret) values ($1, $2) on conflict (id) do update set secret=$2",
    ))
}
pub struct SetTotpDeviceSecretStmt(crate::client::async_::Stmt);
impl SetTotpDeviceSecretStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql>(
        &'s mut self,
        client: &'c C,
        id: &'a uuid::Uuid,
        secret: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[id, secret]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::BytesSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SetTotpDeviceSecretParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SetTotpDeviceSecretStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a SetTotpDeviceSecretParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.id, &params.secret))
    }
}
pub fn get_recovery_code_hash() -> GetRecoveryCodeHashStmt {
    GetRecoveryCodeHashStmt(crate::client::async_::Stmt::new(
        "select code from mfa_recovery_codes where user_id=$1",
    ))
}
pub struct GetRecoveryCodeHashStmt(crate::client::async_::Stmt);
impl GetRecoveryCodeHashStmt {
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
pub fn set_recovery_code_hash() -> SetRecoveryCodeHashStmt {
    SetRecoveryCodeHashStmt(crate::client::async_::Stmt::new(
        "insert into mfa_recovery_codes (user_id, code) values ($1, $2) on conflict (user_id) do update set code=$2",
    ))
}
pub struct SetRecoveryCodeHashStmt(crate::client::async_::Stmt);
impl SetRecoveryCodeHashStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
        code: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[user_id, code]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::BytesSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SetRecoveryCodeHashParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SetRecoveryCodeHashStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a SetRecoveryCodeHashParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.user_id, &params.code))
    }
}
pub fn delete_recovery_code_hash() -> DeleteRecoveryCodeHashStmt {
    DeleteRecoveryCodeHashStmt(crate::client::async_::Stmt::new(
        "delete from mfa_recovery_codes where user_id=$1",
    ))
}
pub struct DeleteRecoveryCodeHashStmt(crate::client::async_::Stmt);
impl DeleteRecoveryCodeHashStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[user_id]).await
    }
}
