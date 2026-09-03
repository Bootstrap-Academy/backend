// This file was generated with `clorinde`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub subject: T1,
    pub reference: Option<T2>,
    pub text_version: T3,
    pub consented_at: chrono::DateTime<chrono::FixedOffset>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Consent {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub subject: String,
    pub reference: Option<String>,
    pub text_version: String,
    pub consented_at: chrono::DateTime<chrono::FixedOffset>,
}
pub struct ConsentBorrowed<'a> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub subject: &'a str,
    pub reference: Option<&'a str>,
    pub text_version: &'a str,
    pub consented_at: chrono::DateTime<chrono::FixedOffset>,
}
impl<'a> From<ConsentBorrowed<'a>> for Consent {
    fn from(
        ConsentBorrowed {
            id,
            user_id,
            subject,
            reference,
            text_version,
            consented_at,
        }: ConsentBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            user_id,
            subject: subject.into(),
            reference: reference.map(|v| v.into()),
            text_version: text_version.into(),
            consented_at,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct ConsentQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<ConsentBorrowed, tokio_postgres::Error>,
    mapper: fn(ConsentBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ConsentQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(ConsentBorrowed) -> R) -> ConsentQuery<'c, 'a, 's, C, R, N> {
        ConsentQuery {
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
pub struct CreateStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn create() -> CreateStmt {
    CreateStmt(
        "insert into withdrawal_consents (id, user_id, subject, reference, text_version, consented_at) values ($1, $2, $3, $4, $5, $6)",
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
        user_id: &'a uuid::Uuid,
        subject: &'a T1,
        reference: &'a Option<T2>,
        text_version: &'a T3,
        consented_at: &'a chrono::DateTime<chrono::FixedOffset>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[id, user_id, subject, reference, text_version, consented_at],
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
            &params.user_id,
            &params.subject,
            &params.reference,
            &params.text_version,
            &params.consented_at,
        ))
    }
}
pub struct ListByUserIdStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_by_user_id() -> ListByUserIdStmt {
    ListByUserIdStmt(
        "select * from withdrawal_consents where user_id=$1 order by consented_at asc",
        None,
    )
}
impl ListByUserIdStmt {
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
    ) -> ConsentQuery<'c, 'a, 's, C, Consent, 1> {
        ConsentQuery {
            client,
            params: [user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ConsentBorrowed, tokio_postgres::Error> {
                    Ok(ConsentBorrowed {
                        id: row.try_get(0)?,
                        user_id: row.try_get(1)?,
                        subject: row.try_get(2)?,
                        reference: row.try_get(3)?,
                        text_version: row.try_get(4)?,
                        consented_at: row.try_get(5)?,
                    })
                },
            mapper: |it| Consent::from(it),
        }
    }
}
