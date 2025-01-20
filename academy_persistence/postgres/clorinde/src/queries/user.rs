// This file was generated with `clorinde`. Do not modify.

#[derive(Debug)]
pub struct CountCompositesParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub name: Option<T1>,
    pub email: Option<T2>,
    pub enabled: Option<bool>,
    pub admin: Option<bool>,
    pub mfa_enabled: Option<bool>,
    pub email_verified: Option<bool>,
    pub newsletter: Option<bool>,
}
#[derive(Debug)]
pub struct ListCompositesParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub name: Option<T1>,
    pub email: Option<T2>,
    pub enabled: Option<bool>,
    pub admin: Option<bool>,
    pub mfa_enabled: Option<bool>,
    pub email_verified: Option<bool>,
    pub newsletter: Option<bool>,
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
pub struct CreateParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub id: uuid::Uuid,
    pub name: T1,
    pub email: Option<T2>,
    pub email_verified: bool,
    pub created_at: crate::types::time::TimestampTz,
    pub last_login: Option<crate::types::time::TimestampTz>,
    pub last_name_change: Option<crate::types::time::TimestampTz>,
    pub enabled: bool,
    pub admin: bool,
    pub newsletter: bool,
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
    pub last_login: Option<crate::types::time::TimestampTz>,
    pub last_name_change: Option<crate::types::time::TimestampTz>,
    pub enabled: Option<bool>,
    pub admin: Option<bool>,
    pub newsletter: Option<bool>,
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
    pub created_at: crate::types::time::TimestampTz,
    pub last_login: Option<crate::types::time::TimestampTz>,
    pub last_name_change: Option<crate::types::time::TimestampTz>,
    pub enabled: bool,
    pub admin: bool,
    pub newsletter: bool,
    pub display_name: String,
    pub bio: String,
    pub tags: Vec<String>,
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
    pub created_at: crate::types::time::TimestampTz,
    pub last_login: Option<crate::types::time::TimestampTz>,
    pub last_name_change: Option<crate::types::time::TimestampTz>,
    pub enabled: bool,
    pub admin: bool,
    pub newsletter: bool,
    pub display_name: &'a str,
    pub bio: &'a str,
    pub tags: crate::ArrayIterator<'a, &'a str>,
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
            newsletter,
            display_name,
            bio,
            tags,
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
            newsletter,
            display_name: display_name.into(),
            bio: bio.into(),
            tags: tags.map(|v| v.into()).collect(),
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
pub struct UserCompositeQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> UserCompositeBorrowed,
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
pub struct BoolQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> bool,
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
pub struct StringQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> &str,
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
pub fn count_composites() -> CountCompositesStmt {
    CountCompositesStmt(crate::client::async_::Stmt::new(
        "select count(*) from user_composites
  where ($1::text is null
    or position(lower($1) in lower(name)) > 0
    or position(lower($1) in lower(display_name)) > 0)
  and ($2::text is null or position(lower($2) in email) > 0)
  and ($3::boolean is null or enabled = $3)
  and ($4::boolean is null or admin = $4)
  and ($5::boolean is null or mfa_enabled = $5)
  and ($6::boolean is null or email_verified = $6)
  and ($7::boolean is null or newsletter = $7)",
    ))
}
pub struct CountCompositesStmt(crate::client::async_::Stmt);
impl CountCompositesStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s mut self,
        client: &'c C,
        name: &'a Option<T1>,
        email: &'a Option<T2>,
        enabled: &'a Option<bool>,
        admin: &'a Option<bool>,
        mfa_enabled: &'a Option<bool>,
        email_verified: &'a Option<bool>,
        newsletter: &'a Option<bool>,
    ) -> I64Query<'c, 'a, 's, C, i64, 7> {
        I64Query {
            client,
            params: [
                name,
                email,
                enabled,
                admin,
                mfa_enabled,
                email_verified,
                newsletter,
            ],
            stmt: &mut self.0,
            extractor: |row| row.get(0),
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
        I64Query<'c, 'a, 's, C, i64, 7>,
        C,
    > for CountCompositesStmt
{
    fn params(
        &'s mut self,
        client: &'c C,
        params: &'a CountCompositesParams<T1, T2>,
    ) -> I64Query<'c, 'a, 's, C, i64, 7> {
        self.bind(
            client,
            &params.name,
            &params.email,
            &params.enabled,
            &params.admin,
            &params.mfa_enabled,
            &params.email_verified,
            &params.newsletter,
        )
    }
}
pub fn list_composites() -> ListCompositesStmt {
    ListCompositesStmt(crate::client::async_::Stmt::new(
        "select * from user_composites
  where ($1::text is null
    or position(lower($1) in lower(name)) > 0
    or position(lower($1) in lower(display_name)) > 0)
  and ($2::text is null or position(lower($2) in email) > 0)
  and ($3::boolean is null or enabled = $3)
  and ($4::boolean is null or admin = $4)
  and ($5::boolean is null or mfa_enabled = $5)
  and ($6::boolean is null or email_verified = $6)
  and ($7::boolean is null or newsletter = $7)
  order by created_at asc
  limit $8 offset $9",
    ))
}
pub struct ListCompositesStmt(crate::client::async_::Stmt);
impl ListCompositesStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s mut self,
        client: &'c C,
        name: &'a Option<T1>,
        email: &'a Option<T2>,
        enabled: &'a Option<bool>,
        admin: &'a Option<bool>,
        mfa_enabled: &'a Option<bool>,
        email_verified: &'a Option<bool>,
        newsletter: &'a Option<bool>,
        limit: &'a i64,
        offset: &'a i64,
    ) -> UserCompositeQuery<'c, 'a, 's, C, UserComposite, 9> {
        UserCompositeQuery {
            client,
            params: [
                name,
                email,
                enabled,
                admin,
                mfa_enabled,
                email_verified,
                newsletter,
                limit,
                offset,
            ],
            stmt: &mut self.0,
            extractor: |row| UserCompositeBorrowed {
                user_id: row.get(0),
                id: row.get(1),
                name: row.get(2),
                email: row.get(3),
                email_verified: row.get(4),
                created_at: row.get(5),
                last_login: row.get(6),
                last_name_change: row.get(7),
                enabled: row.get(8),
                admin: row.get(9),
                newsletter: row.get(10),
                display_name: row.get(11),
                bio: row.get(12),
                tags: row.get(13),
                mfa_enabled: row.get(14),
                password_login: row.get(15),
                oauth2_login: row.get(16),
                business: row.get(17),
                first_name: row.get(18),
                last_name: row.get(19),
                street: row.get(20),
                zip_code: row.get(21),
                city: row.get(22),
                country: row.get(23),
                vat_id: row.get(24),
            },
            mapper: |it| <UserComposite>::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListCompositesParams<T1, T2>,
        UserCompositeQuery<'c, 'a, 's, C, UserComposite, 9>,
        C,
    > for ListCompositesStmt
{
    fn params(
        &'s mut self,
        client: &'c C,
        params: &'a ListCompositesParams<T1, T2>,
    ) -> UserCompositeQuery<'c, 'a, 's, C, UserComposite, 9> {
        self.bind(
            client,
            &params.name,
            &params.email,
            &params.enabled,
            &params.admin,
            &params.mfa_enabled,
            &params.email_verified,
            &params.newsletter,
            &params.limit,
            &params.offset,
        )
    }
}
pub fn exists() -> ExistsStmt {
    ExistsStmt(crate::client::async_::Stmt::new(
        "select (exists (select 1 from users where id=$1))",
    ))
}
pub struct ExistsStmt(crate::client::async_::Stmt);
impl ExistsStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        id: &'a uuid::Uuid,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 1> {
        BoolQuery {
            client,
            params: [id],
            stmt: &mut self.0,
            extractor: |row| row.get(0),
            mapper: |it| it,
        }
    }
}
pub fn get_composite() -> GetCompositeStmt {
    GetCompositeStmt(crate::client::async_::Stmt::new(
        "select * from user_composites where id=$1",
    ))
}
pub struct GetCompositeStmt(crate::client::async_::Stmt);
impl GetCompositeStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        id: &'a uuid::Uuid,
    ) -> UserCompositeQuery<'c, 'a, 's, C, UserComposite, 1> {
        UserCompositeQuery {
            client,
            params: [id],
            stmt: &mut self.0,
            extractor: |row| UserCompositeBorrowed {
                user_id: row.get(0),
                id: row.get(1),
                name: row.get(2),
                email: row.get(3),
                email_verified: row.get(4),
                created_at: row.get(5),
                last_login: row.get(6),
                last_name_change: row.get(7),
                enabled: row.get(8),
                admin: row.get(9),
                newsletter: row.get(10),
                display_name: row.get(11),
                bio: row.get(12),
                tags: row.get(13),
                mfa_enabled: row.get(14),
                password_login: row.get(15),
                oauth2_login: row.get(16),
                business: row.get(17),
                first_name: row.get(18),
                last_name: row.get(19),
                street: row.get(20),
                zip_code: row.get(21),
                city: row.get(22),
                country: row.get(23),
                vat_id: row.get(24),
            },
            mapper: |it| <UserComposite>::from(it),
        }
    }
}
pub fn get_composite_by_name() -> GetCompositeByNameStmt {
    GetCompositeByNameStmt(crate::client::async_::Stmt::new(
        "select * from user_composites where lower(name)=lower($1)",
    ))
}
pub struct GetCompositeByNameStmt(crate::client::async_::Stmt);
impl GetCompositeByNameStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s mut self,
        client: &'c C,
        name: &'a T1,
    ) -> UserCompositeQuery<'c, 'a, 's, C, UserComposite, 1> {
        UserCompositeQuery {
            client,
            params: [name],
            stmt: &mut self.0,
            extractor: |row| UserCompositeBorrowed {
                user_id: row.get(0),
                id: row.get(1),
                name: row.get(2),
                email: row.get(3),
                email_verified: row.get(4),
                created_at: row.get(5),
                last_login: row.get(6),
                last_name_change: row.get(7),
                enabled: row.get(8),
                admin: row.get(9),
                newsletter: row.get(10),
                display_name: row.get(11),
                bio: row.get(12),
                tags: row.get(13),
                mfa_enabled: row.get(14),
                password_login: row.get(15),
                oauth2_login: row.get(16),
                business: row.get(17),
                first_name: row.get(18),
                last_name: row.get(19),
                street: row.get(20),
                zip_code: row.get(21),
                city: row.get(22),
                country: row.get(23),
                vat_id: row.get(24),
            },
            mapper: |it| <UserComposite>::from(it),
        }
    }
}
pub fn get_composite_by_email() -> GetCompositeByEmailStmt {
    GetCompositeByEmailStmt(crate::client::async_::Stmt::new(
        "select * from user_composites where lower(email)=lower($1)",
    ))
}
pub struct GetCompositeByEmailStmt(crate::client::async_::Stmt);
impl GetCompositeByEmailStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s mut self,
        client: &'c C,
        email: &'a T1,
    ) -> UserCompositeQuery<'c, 'a, 's, C, UserComposite, 1> {
        UserCompositeQuery {
            client,
            params: [email],
            stmt: &mut self.0,
            extractor: |row| UserCompositeBorrowed {
                user_id: row.get(0),
                id: row.get(1),
                name: row.get(2),
                email: row.get(3),
                email_verified: row.get(4),
                created_at: row.get(5),
                last_login: row.get(6),
                last_name_change: row.get(7),
                enabled: row.get(8),
                admin: row.get(9),
                newsletter: row.get(10),
                display_name: row.get(11),
                bio: row.get(12),
                tags: row.get(13),
                mfa_enabled: row.get(14),
                password_login: row.get(15),
                oauth2_login: row.get(16),
                business: row.get(17),
                first_name: row.get(18),
                last_name: row.get(19),
                street: row.get(20),
                zip_code: row.get(21),
                city: row.get(22),
                country: row.get(23),
                vat_id: row.get(24),
            },
            mapper: |it| <UserComposite>::from(it),
        }
    }
}
pub fn get_composite_by_oauth2_provider_id_and_remote_user_id(
) -> GetCompositeByOauth2ProviderIdAndRemoteUserIdStmt {
    GetCompositeByOauth2ProviderIdAndRemoteUserIdStmt(crate::client::async_::Stmt::new(
        "with cte as (
  select user_id as id from oauth2_links where provider_id=$1 and remote_user_id=$2
)
select * from user_composites inner join cte using (id)",
    ))
}
pub struct GetCompositeByOauth2ProviderIdAndRemoteUserIdStmt(crate::client::async_::Stmt);
impl GetCompositeByOauth2ProviderIdAndRemoteUserIdStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s mut self,
        client: &'c C,
        provider_id: &'a T1,
        remote_user_id: &'a T2,
    ) -> UserCompositeQuery<'c, 'a, 's, C, UserComposite, 2> {
        UserCompositeQuery {
            client,
            params: [provider_id, remote_user_id],
            stmt: &mut self.0,
            extractor: |row| UserCompositeBorrowed {
                user_id: row.get(1),
                id: row.get(0),
                name: row.get(2),
                email: row.get(3),
                email_verified: row.get(4),
                created_at: row.get(5),
                last_login: row.get(6),
                last_name_change: row.get(7),
                enabled: row.get(8),
                admin: row.get(9),
                newsletter: row.get(10),
                display_name: row.get(11),
                bio: row.get(12),
                tags: row.get(13),
                mfa_enabled: row.get(14),
                password_login: row.get(15),
                oauth2_login: row.get(16),
                business: row.get(17),
                first_name: row.get(18),
                last_name: row.get(19),
                street: row.get(20),
                zip_code: row.get(21),
                city: row.get(22),
                country: row.get(23),
                vat_id: row.get(24),
            },
            mapper: |it| <UserComposite>::from(it),
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
        &'s mut self,
        client: &'c C,
        params: &'a GetCompositeByOauth2ProviderIdAndRemoteUserIdParams<T1, T2>,
    ) -> UserCompositeQuery<'c, 'a, 's, C, UserComposite, 2> {
        self.bind(client, &params.provider_id, &params.remote_user_id)
    }
}
pub fn create() -> CreateStmt {
    CreateStmt(crate::client::async_::Stmt::new("insert into users (id, name, email, email_verified, created_at, last_login, last_name_change, enabled, admin, newsletter)
  values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"))
}
pub struct CreateStmt(crate::client::async_::Stmt);
impl CreateStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s mut self,
        client: &'c C,
        id: &'a uuid::Uuid,
        name: &'a T1,
        email: &'a Option<T2>,
        email_verified: &'a bool,
        created_at: &'a crate::types::time::TimestampTz,
        last_login: &'a Option<crate::types::time::TimestampTz>,
        last_name_change: &'a Option<crate::types::time::TimestampTz>,
        enabled: &'a bool,
        admin: &'a bool,
        newsletter: &'a bool,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client
            .execute(
                stmt,
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
                    newsletter,
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
        CreateParams<T1, T2>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CreateStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a CreateParams<T1, T2>,
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
            &params.newsletter,
        ))
    }
}
pub fn create_profile() -> CreateProfileStmt {
    CreateProfileStmt(crate::client::async_::Stmt::new(
        "insert into user_profiles (user_id, display_name, bio, tags)
  values ($1, $2, $3, $4)",
    ))
}
pub struct CreateProfileStmt(crate::client::async_::Stmt);
impl CreateProfileStmt {
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
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
        display_name: &'a T1,
        bio: &'a T2,
        tags: &'a T4,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client
            .execute(stmt, &[user_id, display_name, bio, tags])
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
        &'a mut self,
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
        ))
    }
}
pub fn create_invoice_info() -> CreateInvoiceInfoStmt {
    CreateInvoiceInfoStmt(crate::client::async_::Stmt::new("insert into user_invoice_info (user_id, business, first_name, last_name, street, zip_code, city, country, vat_id)
  values ($1, $2, $3, $4, $5, $6, $7, $8, $9)"))
}
pub struct CreateInvoiceInfoStmt(crate::client::async_::Stmt);
impl CreateInvoiceInfoStmt {
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
        &'s mut self,
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
        let stmt = self.0.prepare(client).await?;
        client
            .execute(
                stmt,
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
        &'a mut self,
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
pub fn update() -> UpdateStmt {
    UpdateStmt(crate::client::async_::Stmt::new(
        "update users
  set
    name=coalesce($1, name),
    email=coalesce($2, email),
    email_verified=coalesce($3, email_verified),
    last_login=coalesce($4, last_login),
    last_name_change=coalesce($5, last_name_change),
    enabled=coalesce($6, enabled),
    admin=coalesce($7, admin),
    newsletter=coalesce($8, newsletter)
  where id=$9",
    ))
}
pub struct UpdateStmt(crate::client::async_::Stmt);
impl UpdateStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s mut self,
        client: &'c C,
        name: &'a Option<T1>,
        email: &'a Option<T2>,
        email_verified: &'a Option<bool>,
        last_login: &'a Option<crate::types::time::TimestampTz>,
        last_name_change: &'a Option<crate::types::time::TimestampTz>,
        enabled: &'a Option<bool>,
        admin: &'a Option<bool>,
        newsletter: &'a Option<bool>,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client
            .execute(
                stmt,
                &[
                    name,
                    email,
                    email_verified,
                    last_login,
                    last_name_change,
                    enabled,
                    admin,
                    newsletter,
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
        &'a mut self,
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
            &params.newsletter,
            &params.id,
        ))
    }
}
pub fn update_profile() -> UpdateProfileStmt {
    UpdateProfileStmt(crate::client::async_::Stmt::new(
        "update user_profiles
  set
    display_name=coalesce($1, display_name),
    bio=coalesce($2, bio),
    tags=coalesce($3, tags)
  where user_id=$4",
    ))
}
pub struct UpdateProfileStmt(crate::client::async_::Stmt);
impl UpdateProfileStmt {
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
        &'s mut self,
        client: &'c C,
        display_name: &'a Option<T1>,
        bio: &'a Option<T2>,
        tags: &'a Option<T4>,
        user_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client
            .execute(stmt, &[display_name, bio, tags, user_id])
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
        &'a mut self,
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
            &params.user_id,
        ))
    }
}
pub fn update_invoice_info() -> UpdateInvoiceInfoStmt {
    UpdateInvoiceInfoStmt(crate::client::async_::Stmt::new(
        "update user_invoice_info
  set
    business=case when $1 then null else coalesce($2, business) end,
    first_name=case when $3 then null else coalesce($4, first_name) end,
    last_name=case when $5 then null else coalesce($6, last_name) end,
    street=case when $7 then null else coalesce($8, street) end,
    zip_code=case when $9 then null else coalesce($10, zip_code) end,
    city=case when $11 then null else coalesce($12, city) end,
    country=case when $13 then null else coalesce($14, country) end,
    vat_id=case when $15 then null else coalesce($16, vat_id) end
  where user_id=$17",
    ))
}
pub struct UpdateInvoiceInfoStmt(crate::client::async_::Stmt);
impl UpdateInvoiceInfoStmt {
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
        &'s mut self,
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
        let stmt = self.0.prepare(client).await?;
        client
            .execute(
                stmt,
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
        &'a mut self,
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
pub fn delete() -> DeleteStmt {
    DeleteStmt(crate::client::async_::Stmt::new(
        "delete from users where id=$1",
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
pub fn get_password_hash() -> GetPasswordHashStmt {
    GetPasswordHashStmt(crate::client::async_::Stmt::new(
        "select password_hash from user_passwords where user_id=$1",
    ))
}
pub struct GetPasswordHashStmt(crate::client::async_::Stmt);
impl GetPasswordHashStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> StringQuery<'c, 'a, 's, C, String, 1> {
        StringQuery {
            client,
            params: [user_id],
            stmt: &mut self.0,
            extractor: |row| row.get(0),
            mapper: |it| it.into(),
        }
    }
}
pub fn set_password_hash() -> SetPasswordHashStmt {
    SetPasswordHashStmt(crate::client::async_::Stmt::new(
        "insert into user_passwords (user_id, password_hash)
  values ($1, $2)
  on conflict (user_id) do update set password_hash=$2",
    ))
}
pub struct SetPasswordHashStmt(crate::client::async_::Stmt);
impl SetPasswordHashStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
        password_hash: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[user_id, password_hash]).await
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
        &'a mut self,
        client: &'a C,
        params: &'a SetPasswordHashParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.user_id, &params.password_hash))
    }
}
pub fn remove_password_hash() -> RemovePasswordHashStmt {
    RemovePasswordHashStmt(crate::client::async_::Stmt::new(
        "delete from user_passwords where user_id=$1",
    ))
}
pub struct RemovePasswordHashStmt(crate::client::async_::Stmt);
impl RemovePasswordHashStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[user_id]).await
    }
}
pub fn get_number() -> GetNumberStmt {
    GetNumberStmt(crate::client::async_::Stmt::new(
        "merge into user_numbers
  using (select $1::uuid as user_id) s
  on user_numbers.user_id = s.user_id
  when matched then update set number=number
  when not matched then insert (user_id, number) values ($1, nextval('user_number'))
  returning number",
    ))
}
pub struct GetNumberStmt(crate::client::async_::Stmt);
impl GetNumberStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> I64Query<'c, 'a, 's, C, i64, 1> {
        I64Query {
            client,
            params: [user_id],
            stmt: &mut self.0,
            extractor: |row| row.get(0),
            mapper: |it| it,
        }
    }
}
