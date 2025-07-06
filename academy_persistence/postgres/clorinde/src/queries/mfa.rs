// This file was generated with `clorinde`. Do not modify.

#[derive(Clone, Copy, Debug)]
pub struct CreateTotpDeviceParams {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
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
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct TotpDeviceQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<TotpDevice, tokio_postgres::Error>,
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
pub struct ListTotpDevicesByUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_totp_devices_by_user() -> ListTotpDevicesByUserStmt {
    ListTotpDevicesByUserStmt("select * from totp_devices where user_id=$1", None)
}
impl ListTotpDevicesByUserStmt {
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
    ) -> TotpDeviceQuery<'c, 'a, 's, C, TotpDevice, 1> {
        TotpDeviceQuery {
            client,
            params: [user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &tokio_postgres::Row| -> Result<TotpDevice, tokio_postgres::Error> {
                Ok(TotpDevice {
                    id: row.try_get(0)?,
                    user_id: row.try_get(1)?,
                    enabled: row.try_get(2)?,
                    created_at: row.try_get(3)?,
                })
            },
            mapper: |it| TotpDevice::from(it),
        }
    }
}
pub struct CreateTotpDeviceStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn create_totp_device() -> CreateTotpDeviceStmt {
    CreateTotpDeviceStmt(
        "insert into totp_devices (id, user_id, enabled, created_at) values ($1, $2, $3, $4)",
        None,
    )
}
impl CreateTotpDeviceStmt {
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
        user_id: &'a uuid::Uuid,
        enabled: &'a bool,
        created_at: &'a chrono::DateTime<chrono::FixedOffset>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[id, user_id, enabled, created_at])
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
        &'a self,
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
pub struct UpdateTotpDeviceStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update_totp_device() -> UpdateTotpDeviceStmt {
    UpdateTotpDeviceStmt(
        "update totp_devices set enabled=coalesce($1, enabled) where id=$2",
        None,
    )
}
impl UpdateTotpDeviceStmt {
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
        enabled: &'a Option<bool>,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[enabled, id]).await
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
        &'a self,
        client: &'a C,
        params: &'a UpdateTotpDeviceParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.enabled, &params.id))
    }
}
pub struct DeleteTotpDevicesByUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_totp_devices_by_user() -> DeleteTotpDevicesByUserStmt {
    DeleteTotpDevicesByUserStmt("delete from totp_devices where user_id=$1", None)
}
impl DeleteTotpDevicesByUserStmt {
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
pub struct ListEnabledTotpDeviceSecretsByUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_enabled_totp_device_secrets_by_user() -> ListEnabledTotpDeviceSecretsByUserStmt {
    ListEnabledTotpDeviceSecretsByUserStmt(
        "select secret from totp_device_secrets inner join totp_devices using(id) where user_id=$1 and enabled",
        None,
    )
}
impl ListEnabledTotpDeviceSecretsByUserStmt {
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
pub struct GetTotpDeviceSecretStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_totp_device_secret() -> GetTotpDeviceSecretStmt {
    GetTotpDeviceSecretStmt("select secret from totp_device_secrets where id=$1", None)
}
impl GetTotpDeviceSecretStmt {
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
    ) -> Vecu8Query<'c, 'a, 's, C, Vec<u8>, 1> {
        Vecu8Query {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
pub struct SetTotpDeviceSecretStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_totp_device_secret() -> SetTotpDeviceSecretStmt {
    SetTotpDeviceSecretStmt(
        "insert into totp_device_secrets (id, secret) values ($1, $2) on conflict (id) do update set secret=$2",
        None,
    )
}
impl SetTotpDeviceSecretStmt {
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
        id: &'a uuid::Uuid,
        secret: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[id, secret]).await
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
        &'a self,
        client: &'a C,
        params: &'a SetTotpDeviceSecretParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.id, &params.secret))
    }
}
pub struct GetRecoveryCodeHashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_recovery_code_hash() -> GetRecoveryCodeHashStmt {
    GetRecoveryCodeHashStmt("select code from mfa_recovery_codes where user_id=$1", None)
}
impl GetRecoveryCodeHashStmt {
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
pub struct SetRecoveryCodeHashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_recovery_code_hash() -> SetRecoveryCodeHashStmt {
    SetRecoveryCodeHashStmt(
        "insert into mfa_recovery_codes (user_id, code) values ($1, $2) on conflict (user_id) do update set code=$2",
        None,
    )
}
impl SetRecoveryCodeHashStmt {
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
        user_id: &'a uuid::Uuid,
        code: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[user_id, code]).await
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
        &'a self,
        client: &'a C,
        params: &'a SetRecoveryCodeHashParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.user_id, &params.code))
    }
}
pub struct DeleteRecoveryCodeHashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_recovery_code_hash() -> DeleteRecoveryCodeHashStmt {
    DeleteRecoveryCodeHashStmt("delete from mfa_recovery_codes where user_id=$1", None)
}
impl DeleteRecoveryCodeHashStmt {
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
