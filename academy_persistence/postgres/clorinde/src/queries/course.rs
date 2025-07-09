// This file was generated with `clorinde`. Do not modify.

#[derive(Debug)]
pub struct GetCourseUserParams<T1: crate::StringSql> {
    pub course_id: T1,
    pub user_id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpdateCourseUserParams<T1: crate::StringSql> {
    pub course_id: T1,
    pub user_id: uuid::Uuid,
    pub purchased: Option<bool>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct CourseUser {
    pub course_id: String,
    pub user_id: uuid::Uuid,
    pub purchased: bool,
}
pub struct CourseUserBorrowed<'a> {
    pub course_id: &'a str,
    pub user_id: uuid::Uuid,
    pub purchased: bool,
}
impl<'a> From<CourseUserBorrowed<'a>> for CourseUser {
    fn from(
        CourseUserBorrowed {
            course_id,
            user_id,
            purchased,
        }: CourseUserBorrowed<'a>,
    ) -> Self {
        Self {
            course_id: course_id.into(),
            user_id,
            purchased,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct CourseUserQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<CourseUserBorrowed, tokio_postgres::Error>,
    mapper: fn(CourseUserBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> CourseUserQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(CourseUserBorrowed) -> R,
    ) -> CourseUserQuery<'c, 'a, 's, C, R, N> {
        CourseUserQuery {
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
pub struct GetCourseUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_course_user() -> GetCourseUserStmt {
    GetCourseUserStmt(
        "select * from course_users where course_id=$1 and user_id=$2",
        None,
    )
}
impl GetCourseUserStmt {
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
        course_id: &'a T1,
        user_id: &'a uuid::Uuid,
    ) -> CourseUserQuery<'c, 'a, 's, C, CourseUser, 2> {
        CourseUserQuery {
            client,
            params: [course_id, user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<CourseUserBorrowed, tokio_postgres::Error> {
                    Ok(CourseUserBorrowed {
                        course_id: row.try_get(0)?,
                        user_id: row.try_get(1)?,
                        purchased: row.try_get(2)?,
                    })
                },
            mapper: |it| CourseUser::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        GetCourseUserParams<T1>,
        CourseUserQuery<'c, 'a, 's, C, CourseUser, 2>,
        C,
    > for GetCourseUserStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetCourseUserParams<T1>,
    ) -> CourseUserQuery<'c, 'a, 's, C, CourseUser, 2> {
        self.bind(client, &params.course_id, &params.user_id)
    }
}
pub struct UpdateCourseUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update_course_user() -> UpdateCourseUserStmt {
    UpdateCourseUserStmt(
        "insert into course_users as cu (course_id, user_id, purchased) values ($1, $2, coalesce($3, false)) on conflict (course_id, user_id) do update set purchased = coalesce($3, cu.purchased)",
        None,
    )
}
impl UpdateCourseUserStmt {
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
        course_id: &'a T1,
        user_id: &'a uuid::Uuid,
        purchased: &'a Option<bool>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[course_id, user_id, purchased])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpdateCourseUserParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpdateCourseUserStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpdateCourseUserParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.course_id,
            &params.user_id,
            &params.purchased,
        ))
    }
}
