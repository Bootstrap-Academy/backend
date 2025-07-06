// This file was generated with `clorinde`. Do not modify.

#[derive(Debug)]
pub struct CreateLinkParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub provider_id: T1,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub remote_user_id: T2,
    pub remote_user_name: T3,
}
#[derive(Debug, Clone, PartialEq)]
pub struct OAuth2Link {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub provider_id: String,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub remote_user_id: String,
    pub remote_user_name: String,
}
pub struct OAuth2LinkBorrowed<'a> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub provider_id: &'a str,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
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
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<OAuth2LinkBorrowed, tokio_postgres::Error>,
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
pub struct ListLinksByUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_links_by_user() -> ListLinksByUserStmt {
    ListLinksByUserStmt("select * from oauth2_links where user_id=$1", None)
}
impl ListLinksByUserStmt {
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
    ) -> OAuth2LinkQuery<'c, 'a, 's, C, OAuth2Link, 1> {
        OAuth2LinkQuery {
            client,
            params: [user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<OAuth2LinkBorrowed, tokio_postgres::Error> {
                    Ok(OAuth2LinkBorrowed {
                        id: row.try_get(0)?,
                        user_id: row.try_get(1)?,
                        provider_id: row.try_get(2)?,
                        created_at: row.try_get(3)?,
                        remote_user_id: row.try_get(4)?,
                        remote_user_name: row.try_get(5)?,
                    })
                },
            mapper: |it| OAuth2Link::from(it),
        }
    }
}
pub struct GetLinkStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_link() -> GetLinkStmt {
    GetLinkStmt("select * from oauth2_links where id=$1", None)
}
impl GetLinkStmt {
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
    ) -> OAuth2LinkQuery<'c, 'a, 's, C, OAuth2Link, 1> {
        OAuth2LinkQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<OAuth2LinkBorrowed, tokio_postgres::Error> {
                    Ok(OAuth2LinkBorrowed {
                        id: row.try_get(0)?,
                        user_id: row.try_get(1)?,
                        provider_id: row.try_get(2)?,
                        created_at: row.try_get(3)?,
                        remote_user_id: row.try_get(4)?,
                        remote_user_name: row.try_get(5)?,
                    })
                },
            mapper: |it| OAuth2Link::from(it),
        }
    }
}
pub struct CreateLinkStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn create_link() -> CreateLinkStmt {
    CreateLinkStmt(
        "insert into oauth2_links (id, user_id, provider_id, created_at, remote_user_id, remote_user_name) values ($1, $2, $3, $4, $5, $6)",
        None,
    )
}
impl CreateLinkStmt {
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
        user_id: &'a uuid::Uuid,
        provider_id: &'a T1,
        created_at: &'a chrono::DateTime<chrono::FixedOffset>,
        remote_user_id: &'a T2,
        remote_user_name: &'a T3,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
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
        &'a self,
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
pub struct DeleteLinkStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_link() -> DeleteLinkStmt {
    DeleteLinkStmt("delete from oauth2_links where id=$1", None)
}
impl DeleteLinkStmt {
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
