// This file was generated with `clorinde`. Do not modify.

#[derive(Debug)]
pub struct CountCompositesParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub name: Option<T1>,
    pub email: Option<T2>,
    pub enabled: Option<bool>,
    pub admin: Option<bool>,
    pub mfa_enabled: Option<bool>,
    pub email_verified: Option<bool>,
}
#[derive(Debug)]
pub struct ListCompositesParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub name: Option<T1>,
    pub email: Option<T2>,
    pub enabled: Option<bool>,
    pub admin: Option<bool>,
    pub mfa_enabled: Option<bool>,
    pub email_verified: Option<bool>,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug)]
pub struct GetCompositeByOauth2ProviderIdAndRemoteUserIdParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
> {
    pub provider_id: T1,
    pub remote_user_id: T2,
}
#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub id: uuid::Uuid,
    pub name: T1,
    pub email: Option<T2>,
    pub email_verified: bool,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub last_login: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub last_name_change: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub enabled: bool,
    pub admin: bool,
    pub terms_version: Option<T3>,
    pub terms_accepted_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub age_confirmed_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}
#[derive(Debug)]
pub struct CreateProfileParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::ArraySql<Item = T3>,
> {
    pub user_id: uuid::Uuid,
    pub display_name: T1,
    pub bio: T2,
    pub tags: T4,
    pub leaderboard_opt_out: bool,
}
#[derive(Debug)]
pub struct CreateInvoiceInfoParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
> {
    pub user_id: uuid::Uuid,
    pub business: Option<bool>,
    pub first_name: Option<T1>,
    pub last_name: Option<T2>,
    pub street: Option<T3>,
    pub zip_code: Option<T4>,
    pub city: Option<T5>,
    pub country: Option<T6>,
    pub vat_id: Option<T7>,
}
#[derive(Debug)]
pub struct UpdateParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub name: Option<T1>,
    pub email: Option<T2>,
    pub email_verified: Option<bool>,
    pub last_login: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub last_name_change: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub enabled: Option<bool>,
    pub admin: Option<bool>,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpdateProfileParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::ArraySql<Item = T3>,
> {
    pub display_name: Option<T1>,
    pub bio: Option<T2>,
    pub tags: Option<T4>,
    pub leaderboard_opt_out: Option<bool>,
    pub user_id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpdateInvoiceInfoParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
> {
    pub clear_business: bool,
    pub business: Option<bool>,
    pub clear_first_name: bool,
    pub first_name: Option<T1>,
    pub clear_last_name: bool,
    pub last_name: Option<T2>,
    pub clear_street: bool,
    pub street: Option<T3>,
    pub clear_zip_code: bool,
    pub zip_code: Option<T4>,
    pub clear_city: bool,
    pub city: Option<T5>,
    pub clear_country: bool,
    pub country: Option<T6>,
    pub clear_vat_id: bool,
    pub vat_id: Option<T7>,
    pub user_id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpdateTermsAcceptanceParams<T1: crate::StringSql> {
    pub terms_version: T1,
    pub terms_accepted_at: chrono::DateTime<chrono::FixedOffset>,
    pub age_confirmed_at: chrono::DateTime<chrono::FixedOffset>,
    pub id: uuid::Uuid,
}
#[derive(Clone, Copy, Debug)]
pub struct UpdateTermsDeclineParams {
    pub terms_declined_at: chrono::DateTime<chrono::FixedOffset>,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct SetPasswordHashParams<T1: crate::StringSql> {
    pub user_id: uuid::Uuid,
    pub password_hash: T1,
}
#[derive(Debug, Clone, PartialEq)]
pub struct UserComposite {
    pub user_id: uuid::Uuid,
    pub id: uuid::Uuid,
    pub name: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub last_login: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub last_name_change: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub enabled: bool,
    pub admin: bool,
    pub terms_version: Option<String>,
    pub terms_accepted_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub age_confirmed_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub terms_declined_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub display_name: String,
    pub bio: String,
    pub tags: Vec<String>,
    pub leaderboard_opt_out: bool,
    pub mfa_enabled: bool,
    pub password_login: bool,
    pub oauth2_login: bool,
    pub business: Option<bool>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub street: Option<String>,
    pub zip_code: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub vat_id: Option<String>,
}
pub struct UserCompositeBorrowed<'a> {
    pub user_id: uuid::Uuid,
    pub id: uuid::Uuid,
    pub name: &'a str,
    pub email: Option<&'a str>,
    pub email_verified: bool,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub last_login: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub last_name_change: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub enabled: bool,
    pub admin: bool,
    pub terms_version: Option<&'a str>,
    pub terms_accepted_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub age_confirmed_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub terms_declined_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub display_name: &'a str,
    pub bio: &'a str,
    pub tags: crate::ArrayIterator<'a, &'a str>,
    pub leaderboard_opt_out: bool,
    pub mfa_enabled: bool,
    pub password_login: bool,
    pub oauth2_login: bool,
    pub business: Option<bool>,
    pub first_name: Option<&'a str>,
    pub last_name: Option<&'a str>,
    pub street: Option<&'a str>,
    pub zip_code: Option<&'a str>,
    pub city: Option<&'a str>,
    pub country: Option<&'a str>,
    pub vat_id: Option<&'a str>,
}
impl<'a> From<UserCompositeBorrowed<'a>> for UserComposite {
    fn from(
        UserCompositeBorrowed {
            user_id,
            id,
            name,
            email,
            email_verified,
            created_at,
            last_login,
            last_name_change,
            enabled,
            admin,
            terms_version,
            terms_accepted_at,
            age_confirmed_at,
            terms_declined_at,
            display_name,
            bio,
            tags,
            leaderboard_opt_out,
            mfa_enabled,
            password_login,
            oauth2_login,
            business,
            first_name,
            last_name,
            street,
            zip_code,
            city,
            country,
            vat_id,
        }: UserCompositeBorrowed<'a>,
    ) -> Self {
        Self {
            user_id,
            id,
            name: name.into(),
            email: email.map(|v| v.into()),
            email_verified,
            created_at,
            last_login,
            last_name_change,
            enabled,
            admin,
            terms_version: terms_version.map(|v| v.into()),
            terms_accepted_at,
            age_confirmed_at,
            terms_declined_at,
            display_name: display_name.into(),
            bio: bio.into(),
            tags: tags.map(|v| v.into()).collect(),
            leaderboard_opt_out,
            mfa_enabled,
            password_login,
            oauth2_login,
            business,
            first_name: first_name.map(|v| v.into()),
            last_name: last_name.map(|v| v.into()),
            street: street.map(|v| v.into()),
            zip_code: zip_code.map(|v| v.into()),
            city: city.map(|v| v.into()),
            country: country.map(|v| v.into()),
            vat_id: vat_id.map(|v| v.into()),
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
pub struct UserCompositeQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<UserCompositeBorrowed, tokio_postgres::Error>,
    mapper: fn(UserCompositeBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> UserCompositeQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(UserCompositeBorrowed) -> R,
    ) -> UserCompositeQuery<'c, 'a, 's, C, R, N> {
        UserCompositeQuery {
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
pub struct BoolQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<bool, tokio_postgres::Error>,
    mapper: fn(bool) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BoolQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(bool) -> R) -> BoolQuery<'c, 'a, 's, C, R, N> {
        BoolQuery {
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
pub struct StringQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<&str, tokio_postgres::Error>,
    mapper: fn(&str) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> StringQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(&str) -> R) -> StringQuery<'c, 'a, 's, C, R, N> {
        StringQuery {
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
pub struct CountCompositesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_composites() -> CountCompositesStmt {
    CountCompositesStmt(
        "select count(*) from user_composites where ($1::text is null or position(lower($1) in lower(name)) > 0 or position(lower($1) in lower(display_name)) > 0) and ($2::text is null or position(lower($2) in email) > 0) and ($3::boolean is null or enabled = $3) and ($4::boolean is null or admin = $4) and ($5::boolean is null or mfa_enabled = $5) and ($6::boolean is null or email_verified = $6)",
        None,
    )
}
impl CountCompositesStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        name: &'a Option<T1>,
        email: &'a Option<T2>,
        enabled: &'a Option<bool>,
        admin: &'a Option<bool>,
        mfa_enabled: &'a Option<bool>,
        email_verified: &'a Option<bool>,
    ) -> I64Query<'c, 'a, 's, C, i64, 6> {
        I64Query {
            client,
            params: [name, email, enabled, admin, mfa_enabled, email_verified],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CountCompositesParams<T1, T2>,
        I64Query<'c, 'a, 's, C, i64, 6>,
        C,
    > for CountCompositesStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CountCompositesParams<T1, T2>,
    ) -> I64Query<'c, 'a, 's, C, i64, 6> {
        self.bind(
            client,
            &params.name,
            &params.email,
            &params.enabled,
            &params.admin,
            &params.mfa_enabled,
            &params.email_verified,
        )
    }
}
pub struct ListCompositesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_composites() -> ListCompositesStmt {
    ListCompositesStmt(
        "select * from user_composites where ($1::text is null or position(lower($1) in lower(name)) > 0 or position(lower($1) in lower(display_name)) > 0) and ($2::text is null or position(lower($2) in email) > 0) and ($3::boolean is null or enabled = $3) and ($4::boolean is null or admin = $4) and ($5::boolean is null or mfa_enabled = $5) and ($6::boolean is null or email_verified = $6) order by created_at asc limit $7 offset $8",
        None,
    )
}
impl ListCompositesStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        name: &'a Option<T1>,
        email: &'a Option<T2>,
        enabled: &'a Option<bool>,
        admin: &'a Option<bool>,
        mfa_enabled: &'a Option<bool>,
        email_verified: &'a Option<bool>,
        limit: &'a i64,
        offset: &'a i64,
    ) -> UserCompositeQuery<'c, 'a, 's, C, UserComposite, 8> {
        UserCompositeQuery {
            client,
            params: [
                name,
                email,
                enabled,
                admin,
                mfa_enabled,
                email_verified,
                limit,
                offset,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserCompositeBorrowed, tokio_postgres::Error> {
                    Ok(UserCompositeBorrowed {
                        user_id: row.try_get(0)?,
                        id: row.try_get(1)?,
                        name: row.try_get(2)?,
                        email: row.try_get(3)?,
                        email_verified: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        last_login: row.try_get(6)?,
                        last_name_change: row.try_get(7)?,
                        enabled: row.try_get(8)?,
                        admin: row.try_get(9)?,
                        terms_version: row.try_get(10)?,
                        terms_accepted_at: row.try_get(11)?,
                        age_confirmed_at: row.try_get(12)?,
                        terms_declined_at: row.try_get(13)?,
                        display_name: row.try_get(14)?,
                        bio: row.try_get(15)?,
                        tags: row.try_get(16)?,
                        leaderboard_opt_out: row.try_get(17)?,
                        mfa_enabled: row.try_get(18)?,
                        password_login: row.try_get(19)?,
                        oauth2_login: row.try_get(20)?,
                        business: row.try_get(21)?,
                        first_name: row.try_get(22)?,
                        last_name: row.try_get(23)?,
                        street: row.try_get(24)?,
                        zip_code: row.try_get(25)?,
                        city: row.try_get(26)?,
                        country: row.try_get(27)?,
                        vat_id: row.try_get(28)?,
                    })
                },
            mapper: |it| UserComposite::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListCompositesParams<T1, T2>,
        UserCompositeQuery<'c, 'a, 's, C, UserComposite, 8>,
        C,
    > for ListCompositesStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListCompositesParams<T1, T2>,
    ) -> UserCompositeQuery<'c, 'a, 's, C, UserComposite, 8> {
        self.bind(
            client,
            &params.name,
            &params.email,
            &params.enabled,
            &params.admin,
            &params.mfa_enabled,
            &params.email_verified,
            &params.limit,
            &params.offset,
        )
    }
}
pub struct ExistsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn exists() -> ExistsStmt {
    ExistsStmt("select (exists (select 1 from users where id=$1))", None)
}
impl ExistsStmt {
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
    ) -> BoolQuery<'c, 'a, 's, C, bool, 1> {
        BoolQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct GetCompositeStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_composite() -> GetCompositeStmt {
    GetCompositeStmt("select * from user_composites where id=$1", None)
}
impl GetCompositeStmt {
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
    ) -> UserCompositeQuery<'c, 'a, 's, C, UserComposite, 1> {
        UserCompositeQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserCompositeBorrowed, tokio_postgres::Error> {
                    Ok(UserCompositeBorrowed {
                        user_id: row.try_get(0)?,
                        id: row.try_get(1)?,
                        name: row.try_get(2)?,
                        email: row.try_get(3)?,
                        email_verified: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        last_login: row.try_get(6)?,
                        last_name_change: row.try_get(7)?,
                        enabled: row.try_get(8)?,
                        admin: row.try_get(9)?,
                        terms_version: row.try_get(10)?,
                        terms_accepted_at: row.try_get(11)?,
                        age_confirmed_at: row.try_get(12)?,
                        terms_declined_at: row.try_get(13)?,
                        display_name: row.try_get(14)?,
                        bio: row.try_get(15)?,
                        tags: row.try_get(16)?,
                        leaderboard_opt_out: row.try_get(17)?,
                        mfa_enabled: row.try_get(18)?,
                        password_login: row.try_get(19)?,
                        oauth2_login: row.try_get(20)?,
                        business: row.try_get(21)?,
                        first_name: row.try_get(22)?,
                        last_name: row.try_get(23)?,
                        street: row.try_get(24)?,
                        zip_code: row.try_get(25)?,
                        city: row.try_get(26)?,
                        country: row.try_get(27)?,
                        vat_id: row.try_get(28)?,
                    })
                },
            mapper: |it| UserComposite::from(it),
        }
    }
}
pub struct GetCompositeByNameStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_composite_by_name() -> GetCompositeByNameStmt {
    GetCompositeByNameStmt(
        "select * from user_composites where lower(name)=lower($1)",
        None,
    )
}
impl GetCompositeByNameStmt {
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
        name: &'a T1,
    ) -> UserCompositeQuery<'c, 'a, 's, C, UserComposite, 1> {
        UserCompositeQuery {
            client,
            params: [name],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserCompositeBorrowed, tokio_postgres::Error> {
                    Ok(UserCompositeBorrowed {
                        user_id: row.try_get(0)?,
                        id: row.try_get(1)?,
                        name: row.try_get(2)?,
                        email: row.try_get(3)?,
                        email_verified: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        last_login: row.try_get(6)?,
                        last_name_change: row.try_get(7)?,
                        enabled: row.try_get(8)?,
                        admin: row.try_get(9)?,
                        terms_version: row.try_get(10)?,
                        terms_accepted_at: row.try_get(11)?,
                        age_confirmed_at: row.try_get(12)?,
                        terms_declined_at: row.try_get(13)?,
                        display_name: row.try_get(14)?,
                        bio: row.try_get(15)?,
                        tags: row.try_get(16)?,
                        leaderboard_opt_out: row.try_get(17)?,
                        mfa_enabled: row.try_get(18)?,
                        password_login: row.try_get(19)?,
                        oauth2_login: row.try_get(20)?,
                        business: row.try_get(21)?,
                        first_name: row.try_get(22)?,
                        last_name: row.try_get(23)?,
                        street: row.try_get(24)?,
                        zip_code: row.try_get(25)?,
                        city: row.try_get(26)?,
                        country: row.try_get(27)?,
                        vat_id: row.try_get(28)?,
                    })
                },
            mapper: |it| UserComposite::from(it),
        }
    }
}
pub struct GetCompositeByEmailStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_composite_by_email() -> GetCompositeByEmailStmt {
    GetCompositeByEmailStmt(
        "select * from user_composites where lower(email)=lower($1)",
        None,
    )
}
impl GetCompositeByEmailStmt {
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
        email: &'a T1,
    ) -> UserCompositeQuery<'c, 'a, 's, C, UserComposite, 1> {
        UserCompositeQuery {
            client,
            params: [email],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserCompositeBorrowed, tokio_postgres::Error> {
                    Ok(UserCompositeBorrowed {
                        user_id: row.try_get(0)?,
                        id: row.try_get(1)?,
                        name: row.try_get(2)?,
                        email: row.try_get(3)?,
                        email_verified: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        last_login: row.try_get(6)?,
                        last_name_change: row.try_get(7)?,
                        enabled: row.try_get(8)?,
                        admin: row.try_get(9)?,
                        terms_version: row.try_get(10)?,
                        terms_accepted_at: row.try_get(11)?,
                        age_confirmed_at: row.try_get(12)?,
                        terms_declined_at: row.try_get(13)?,
                        display_name: row.try_get(14)?,
                        bio: row.try_get(15)?,
                        tags: row.try_get(16)?,
                        leaderboard_opt_out: row.try_get(17)?,
                        mfa_enabled: row.try_get(18)?,
                        password_login: row.try_get(19)?,
                        oauth2_login: row.try_get(20)?,
                        business: row.try_get(21)?,
                        first_name: row.try_get(22)?,
                        last_name: row.try_get(23)?,
                        street: row.try_get(24)?,
                        zip_code: row.try_get(25)?,
                        city: row.try_get(26)?,
                        country: row.try_get(27)?,
                        vat_id: row.try_get(28)?,
                    })
                },
            mapper: |it| UserComposite::from(it),
        }
    }
}
pub struct GetCompositeByOauth2ProviderIdAndRemoteUserIdStmt(
    &'static str,
    Option<tokio_postgres::Statement>,
);
pub fn get_composite_by_oauth2_provider_id_and_remote_user_id()
-> GetCompositeByOauth2ProviderIdAndRemoteUserIdStmt {
    GetCompositeByOauth2ProviderIdAndRemoteUserIdStmt(
        "with cte as ( select user_id as id from oauth2_links where provider_id=$1 and remote_user_id=$2 ) select * from user_composites inner join cte using (id)",
        None,
    )
}
impl GetCompositeByOauth2ProviderIdAndRemoteUserIdStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        provider_id: &'a T1,
        remote_user_id: &'a T2,
    ) -> UserCompositeQuery<'c, 'a, 's, C, UserComposite, 2> {
        UserCompositeQuery {
            client,
            params: [provider_id, remote_user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserCompositeBorrowed, tokio_postgres::Error> {
                    Ok(UserCompositeBorrowed {
                        user_id: row.try_get(1)?,
                        id: row.try_get(0)?,
                        name: row.try_get(2)?,
                        email: row.try_get(3)?,
                        email_verified: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        last_login: row.try_get(6)?,
                        last_name_change: row.try_get(7)?,
                        enabled: row.try_get(8)?,
                        admin: row.try_get(9)?,
                        terms_version: row.try_get(10)?,
                        terms_accepted_at: row.try_get(11)?,
                        age_confirmed_at: row.try_get(12)?,
                        terms_declined_at: row.try_get(13)?,
                        display_name: row.try_get(14)?,
                        bio: row.try_get(15)?,
                        tags: row.try_get(16)?,
                        leaderboard_opt_out: row.try_get(17)?,
                        mfa_enabled: row.try_get(18)?,
                        password_login: row.try_get(19)?,
                        oauth2_login: row.try_get(20)?,
                        business: row.try_get(21)?,
                        first_name: row.try_get(22)?,
                        last_name: row.try_get(23)?,
                        street: row.try_get(24)?,
                        zip_code: row.try_get(25)?,
                        city: row.try_get(26)?,
                        country: row.try_get(27)?,
                        vat_id: row.try_get(28)?,
                    })
                },
            mapper: |it| UserComposite::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        GetCompositeByOauth2ProviderIdAndRemoteUserIdParams<T1, T2>,
        UserCompositeQuery<'c, 'a, 's, C, UserComposite, 2>,
        C,
    > for GetCompositeByOauth2ProviderIdAndRemoteUserIdStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetCompositeByOauth2ProviderIdAndRemoteUserIdParams<T1, T2>,
    ) -> UserCompositeQuery<'c, 'a, 's, C, UserComposite, 2> {
        self.bind(client, &params.provider_id, &params.remote_user_id)
    }
}
pub struct CreateStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn create() -> CreateStmt {
    CreateStmt(
        "insert into users (id, name, email, email_verified, created_at, last_login, last_name_change, enabled, admin, terms_version, terms_accepted_at, age_confirmed_at) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
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
    pub async fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::StringSql,
        T2: crate::StringSql,
        T3: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        id: &'a uuid::Uuid,
        name: &'a T1,
        email: &'a Option<T2>,
        email_verified: &'a bool,
        created_at: &'a chrono::DateTime<chrono::FixedOffset>,
        last_login: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
        last_name_change: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
        enabled: &'a bool,
        admin: &'a bool,
        terms_version: &'a Option<T3>,
        terms_accepted_at: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
        age_confirmed_at: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    id,
                    name,
                    email,
                    email_verified,
                    created_at,
                    last_login,
                    last_name_change,
                    enabled,
                    admin,
                    terms_version,
                    terms_accepted_at,
                    age_confirmed_at,
                ],
            )
            .await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        CreateParams<T1, T2, T3>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CreateStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a CreateParams<T1, T2, T3>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.id,
            &params.name,
            &params.email,
            &params.email_verified,
            &params.created_at,
            &params.last_login,
            &params.last_name_change,
            &params.enabled,
            &params.admin,
            &params.terms_version,
            &params.terms_accepted_at,
            &params.age_confirmed_at,
        ))
    }
}
pub struct CreateProfileStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn create_profile() -> CreateProfileStmt {
    CreateProfileStmt(
        "insert into user_profiles (user_id, display_name, bio, tags, leaderboard_opt_out) values ($1, $2, $3, $4, $5)",
        None,
    )
}
impl CreateProfileStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::StringSql,
        T2: crate::StringSql,
        T3: crate::StringSql,
        T4: crate::ArraySql<Item = T3>,
    >(
        &'s self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
        display_name: &'a T1,
        bio: &'a T2,
        tags: &'a T4,
        leaderboard_opt_out: &'a bool,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[user_id, display_name, bio, tags, leaderboard_opt_out],
            )
            .await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::ArraySql<Item = T3>,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        CreateProfileParams<T1, T2, T3, T4>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CreateProfileStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a CreateProfileParams<T1, T2, T3, T4>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.user_id,
            &params.display_name,
            &params.bio,
            &params.tags,
            &params.leaderboard_opt_out,
        ))
    }
}
pub struct CreateInvoiceInfoStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn create_invoice_info() -> CreateInvoiceInfoStmt {
    CreateInvoiceInfoStmt(
        "insert into user_invoice_info (user_id, business, first_name, last_name, street, zip_code, city, country, vat_id) values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        None,
    )
}
impl CreateInvoiceInfoStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::StringSql,
        T2: crate::StringSql,
        T3: crate::StringSql,
        T4: crate::StringSql,
        T5: crate::StringSql,
        T6: crate::StringSql,
        T7: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
        business: &'a Option<bool>,
        first_name: &'a Option<T1>,
        last_name: &'a Option<T2>,
        street: &'a Option<T3>,
        zip_code: &'a Option<T4>,
        city: &'a Option<T5>,
        country: &'a Option<T6>,
        vat_id: &'a Option<T7>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    user_id, business, first_name, last_name, street, zip_code, city, country,
                    vat_id,
                ],
            )
            .await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        CreateInvoiceInfoParams<T1, T2, T3, T4, T5, T6, T7>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CreateInvoiceInfoStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a CreateInvoiceInfoParams<T1, T2, T3, T4, T5, T6, T7>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.user_id,
            &params.business,
            &params.first_name,
            &params.last_name,
            &params.street,
            &params.zip_code,
            &params.city,
            &params.country,
            &params.vat_id,
        ))
    }
}
pub struct UpdateStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update() -> UpdateStmt {
    UpdateStmt(
        "update users set name=coalesce($1, name), email=coalesce($2, email), email_verified=coalesce($3, email_verified), last_login=coalesce($4, last_login), last_name_change=coalesce($5, last_name_change), enabled=coalesce($6, enabled), admin=coalesce($7, admin) where id=$8",
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
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        name: &'a Option<T1>,
        email: &'a Option<T2>,
        email_verified: &'a Option<bool>,
        last_login: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
        last_name_change: &'a Option<chrono::DateTime<chrono::FixedOffset>>,
        enabled: &'a Option<bool>,
        admin: &'a Option<bool>,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    name,
                    email,
                    email_verified,
                    last_login,
                    last_name_change,
                    enabled,
                    admin,
                    id,
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
        UpdateParams<T1, T2>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpdateStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpdateParams<T1, T2>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.name,
            &params.email,
            &params.email_verified,
            &params.last_login,
            &params.last_name_change,
            &params.enabled,
            &params.admin,
            &params.id,
        ))
    }
}
pub struct UpdateProfileStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update_profile() -> UpdateProfileStmt {
    UpdateProfileStmt(
        "update user_profiles set display_name=coalesce($1, display_name), bio=coalesce($2, bio), tags=coalesce($3, tags), leaderboard_opt_out=coalesce($4, leaderboard_opt_out) where user_id=$5",
        None,
    )
}
impl UpdateProfileStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::StringSql,
        T2: crate::StringSql,
        T3: crate::StringSql,
        T4: crate::ArraySql<Item = T3>,
    >(
        &'s self,
        client: &'c C,
        display_name: &'a Option<T1>,
        bio: &'a Option<T2>,
        tags: &'a Option<T4>,
        leaderboard_opt_out: &'a Option<bool>,
        user_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[display_name, bio, tags, leaderboard_opt_out, user_id],
            )
            .await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::ArraySql<Item = T3>,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpdateProfileParams<T1, T2, T3, T4>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpdateProfileStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpdateProfileParams<T1, T2, T3, T4>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.display_name,
            &params.bio,
            &params.tags,
            &params.leaderboard_opt_out,
            &params.user_id,
        ))
    }
}
pub struct UpdateInvoiceInfoStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update_invoice_info() -> UpdateInvoiceInfoStmt {
    UpdateInvoiceInfoStmt(
        "update user_invoice_info set business=case when $1 then null else coalesce($2, business) end, first_name=case when $3 then null else coalesce($4, first_name) end, last_name=case when $5 then null else coalesce($6, last_name) end, street=case when $7 then null else coalesce($8, street) end, zip_code=case when $9 then null else coalesce($10, zip_code) end, city=case when $11 then null else coalesce($12, city) end, country=case when $13 then null else coalesce($14, country) end, vat_id=case when $15 then null else coalesce($16, vat_id) end where user_id=$17",
        None,
    )
}
impl UpdateInvoiceInfoStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::StringSql,
        T2: crate::StringSql,
        T3: crate::StringSql,
        T4: crate::StringSql,
        T5: crate::StringSql,
        T6: crate::StringSql,
        T7: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        clear_business: &'a bool,
        business: &'a Option<bool>,
        clear_first_name: &'a bool,
        first_name: &'a Option<T1>,
        clear_last_name: &'a bool,
        last_name: &'a Option<T2>,
        clear_street: &'a bool,
        street: &'a Option<T3>,
        clear_zip_code: &'a bool,
        zip_code: &'a Option<T4>,
        clear_city: &'a bool,
        city: &'a Option<T5>,
        clear_country: &'a bool,
        country: &'a Option<T6>,
        clear_vat_id: &'a bool,
        vat_id: &'a Option<T7>,
        user_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    clear_business,
                    business,
                    clear_first_name,
                    first_name,
                    clear_last_name,
                    last_name,
                    clear_street,
                    street,
                    clear_zip_code,
                    zip_code,
                    clear_city,
                    city,
                    clear_country,
                    country,
                    clear_vat_id,
                    vat_id,
                    user_id,
                ],
            )
            .await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpdateInvoiceInfoParams<T1, T2, T3, T4, T5, T6, T7>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpdateInvoiceInfoStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpdateInvoiceInfoParams<T1, T2, T3, T4, T5, T6, T7>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.clear_business,
            &params.business,
            &params.clear_first_name,
            &params.first_name,
            &params.clear_last_name,
            &params.last_name,
            &params.clear_street,
            &params.street,
            &params.clear_zip_code,
            &params.zip_code,
            &params.clear_city,
            &params.city,
            &params.clear_country,
            &params.country,
            &params.clear_vat_id,
            &params.vat_id,
            &params.user_id,
        ))
    }
}
pub struct UpdateTermsAcceptanceStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update_terms_acceptance() -> UpdateTermsAcceptanceStmt {
    UpdateTermsAcceptanceStmt(
        "update users set terms_version=$1, terms_accepted_at=$2, age_confirmed_at=$3, terms_declined_at=null where id=$4",
        None,
    )
}
impl UpdateTermsAcceptanceStmt {
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
        terms_version: &'a T1,
        terms_accepted_at: &'a chrono::DateTime<chrono::FixedOffset>,
        age_confirmed_at: &'a chrono::DateTime<chrono::FixedOffset>,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[terms_version, terms_accepted_at, age_confirmed_at, id],
            )
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpdateTermsAcceptanceParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpdateTermsAcceptanceStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpdateTermsAcceptanceParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.terms_version,
            &params.terms_accepted_at,
            &params.age_confirmed_at,
            &params.id,
        ))
    }
}
pub struct UpdateTermsDeclineStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update_terms_decline() -> UpdateTermsDeclineStmt {
    UpdateTermsDeclineStmt("update users set terms_declined_at=$1 where id=$2", None)
}
impl UpdateTermsDeclineStmt {
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
        terms_declined_at: &'a chrono::DateTime<chrono::FixedOffset>,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[terms_declined_at, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpdateTermsDeclineParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpdateTermsDeclineStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpdateTermsDeclineParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.terms_declined_at, &params.id))
    }
}
pub struct DeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete() -> DeleteStmt {
    DeleteStmt("delete from users where id=$1", None)
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
pub struct GetPasswordHashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_password_hash() -> GetPasswordHashStmt {
    GetPasswordHashStmt(
        "select password_hash from user_passwords where user_id=$1",
        None,
    )
}
impl GetPasswordHashStmt {
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
    ) -> StringQuery<'c, 'a, 's, C, String, 1> {
        StringQuery {
            client,
            params: [user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
pub struct SetPasswordHashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_password_hash() -> SetPasswordHashStmt {
    SetPasswordHashStmt(
        "insert into user_passwords (user_id, password_hash) values ($1, $2) on conflict (user_id) do update set password_hash=$2",
        None,
    )
}
impl SetPasswordHashStmt {
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
        user_id: &'a uuid::Uuid,
        password_hash: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[user_id, password_hash]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SetPasswordHashParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SetPasswordHashStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a SetPasswordHashParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.user_id, &params.password_hash))
    }
}
pub struct RemovePasswordHashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn remove_password_hash() -> RemovePasswordHashStmt {
    RemovePasswordHashStmt("delete from user_passwords where user_id=$1", None)
}
impl RemovePasswordHashStmt {
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
pub struct GetNumberStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_number() -> GetNumberStmt {
    GetNumberStmt(
        "merge into user_numbers using (select $1::uuid as user_id) s on user_numbers.user_id = s.user_id when matched then update set number=number when not matched then insert (user_id, number) values ($1, nextval('user_number')) returning number",
        None,
    )
}
impl GetNumberStmt {
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
    ) -> I64Query<'c, 'a, 's, C, i64, 1> {
        I64Query {
            client,
            params: [user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
