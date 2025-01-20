// This file was generated with `clorinde`. Do not modify.

#[derive(Debug)]
pub struct CreateLinkParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub provider_id: T1,
    pub created_at: crate::types::time::TimestampTz,
    pub remote_user_id: T2,
    pub remote_user_name: T3,
}
#[derive(Debug, Clone, PartialEq)]
pub struct OAuth2Link {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub provider_id: String,
    pub created_at: crate::types::time::TimestampTz,
    pub remote_user_id: String,
    pub remote_user_name: String,
}
pub struct OAuth2LinkBorrowed<'a> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub provider_id: &'a str,
    pub created_at: crate::types::time::TimestampTz,
    pub remote_user_id: &'a str,
    pub remote_user_name: &'a str,
}
impl<'a> From<OAuth2LinkBorrowed<'a>> for OAuth2Link {
    fn from(
        OAuth2LinkBorrowed {
            id,
            user_id,
            provider_id,
            created_at,
            remote_user_id,
            remote_user_name,
        }: OAuth2LinkBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            user_id,
            provider_id: provider_id.into(),
            created_at,
            remote_user_id: remote_user_id.into(),
            remote_user_name: remote_user_name.into(),
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct OAuth2LinkQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    stmt: &'s mut crate::client::async_::Stmt,
    extractor: fn(&tokio_postgres::Row) -> OAuth2LinkBorrowed,
    mapper: fn(OAuth2LinkBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> OAuth2LinkQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(OAuth2LinkBorrowed) -> R,
    ) -> OAuth2LinkQuery<'c, 'a, 's, C, R, N> {
        OAuth2LinkQuery {
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
pub fn list_links_by_user() -> ListLinksByUserStmt {
    ListLinksByUserStmt(crate::client::async_::Stmt::new(
        "select * from oauth2_links where user_id=$1",
    ))
}
pub struct ListLinksByUserStmt(crate::client::async_::Stmt);
impl ListLinksByUserStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
    ) -> OAuth2LinkQuery<'c, 'a, 's, C, OAuth2Link, 1> {
        OAuth2LinkQuery {
            client,
            params: [user_id],
            stmt: &mut self.0,
            extractor: |row| OAuth2LinkBorrowed {
                id: row.get(0),
                user_id: row.get(1),
                provider_id: row.get(2),
                created_at: row.get(3),
                remote_user_id: row.get(4),
                remote_user_name: row.get(5),
            },
            mapper: |it| <OAuth2Link>::from(it),
        }
    }
}
pub fn get_link() -> GetLinkStmt {
    GetLinkStmt(crate::client::async_::Stmt::new(
        "select * from oauth2_links where id=$1",
    ))
}
pub struct GetLinkStmt(crate::client::async_::Stmt);
impl GetLinkStmt {
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        id: &'a uuid::Uuid,
    ) -> OAuth2LinkQuery<'c, 'a, 's, C, OAuth2Link, 1> {
        OAuth2LinkQuery {
            client,
            params: [id],
            stmt: &mut self.0,
            extractor: |row| OAuth2LinkBorrowed {
                id: row.get(0),
                user_id: row.get(1),
                provider_id: row.get(2),
                created_at: row.get(3),
                remote_user_id: row.get(4),
                remote_user_name: row.get(5),
            },
            mapper: |it| <OAuth2Link>::from(it),
        }
    }
}
pub fn create_link() -> CreateLinkStmt {
    CreateLinkStmt(crate::client::async_::Stmt::new("insert into oauth2_links (id, user_id, provider_id, created_at, remote_user_id, remote_user_name)
  values ($1, $2, $3, $4, $5, $6)"))
}
pub struct CreateLinkStmt(crate::client::async_::Stmt);
impl CreateLinkStmt {
    pub async fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::StringSql,
        T2: crate::StringSql,
        T3: crate::StringSql,
    >(
        &'s mut self,
        client: &'c C,
        id: &'a uuid::Uuid,
        user_id: &'a uuid::Uuid,
        provider_id: &'a T1,
        created_at: &'a crate::types::time::TimestampTz,
        remote_user_id: &'a T2,
        remote_user_name: &'a T3,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client
            .execute(
                stmt,
                &[
                    id,
                    user_id,
                    provider_id,
                    created_at,
                    remote_user_id,
                    remote_user_name,
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
        CreateLinkParams<T1, T2, T3>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CreateLinkStmt
{
    fn params(
        &'a mut self,
        client: &'a C,
        params: &'a CreateLinkParams<T1, T2, T3>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.id,
            &params.user_id,
            &params.provider_id,
            &params.created_at,
            &params.remote_user_id,
            &params.remote_user_name,
        ))
    }
}
pub fn delete_link() -> DeleteLinkStmt {
    DeleteLinkStmt(crate::client::async_::Stmt::new(
        "delete from oauth2_links where id=$1",
    ))
}
pub struct DeleteLinkStmt(crate::client::async_::Stmt);
impl DeleteLinkStmt {
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s mut self,
        client: &'c C,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        let stmt = self.0.prepare(client).await?;
        client.execute(stmt, &[id]).await
    }
}
