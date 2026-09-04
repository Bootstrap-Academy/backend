// This file was generated with `clorinde`. Do not modify.

#[derive(Debug)]
pub struct RecordDocumentParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::ArraySql<Item = T3>,
> {
    pub number: T1,
    pub kind: T2,
    pub user_id: Option<uuid::Uuid>,
    pub issued_at: chrono::DateTime<chrono::FixedOffset>,
    pub customer_details: Option<T4>,
    pub coins: Option<i64>,
    pub net_total_cents: Option<i64>,
    pub vat_total_cents: Option<i64>,
    pub gross_total_cents: Option<i64>,
}
#[derive(Debug)]
pub struct PseudonymizeDocumentsParams<T1: crate::StringSql, T2: crate::ArraySql<Item = T1>> {
    pub customer_details: T2,
    pub user_id: uuid::Uuid,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub number: String,
    pub kind: String,
    pub user_id: Option<uuid::Uuid>,
    pub issued_at: chrono::DateTime<chrono::FixedOffset>,
    pub customer_details: Option<Vec<String>>,
    pub coins: Option<i64>,
    pub net_total_cents: Option<i64>,
    pub vat_total_cents: Option<i64>,
    pub gross_total_cents: Option<i64>,
}
pub struct DocumentBorrowed<'a> {
    pub number: &'a str,
    pub kind: &'a str,
    pub user_id: Option<uuid::Uuid>,
    pub issued_at: chrono::DateTime<chrono::FixedOffset>,
    pub customer_details: Option<crate::ArrayIterator<'a, &'a str>>,
    pub coins: Option<i64>,
    pub net_total_cents: Option<i64>,
    pub vat_total_cents: Option<i64>,
    pub gross_total_cents: Option<i64>,
}
impl<'a> From<DocumentBorrowed<'a>> for Document {
    fn from(
        DocumentBorrowed {
            number,
            kind,
            user_id,
            issued_at,
            customer_details,
            coins,
            net_total_cents,
            vat_total_cents,
            gross_total_cents,
        }: DocumentBorrowed<'a>,
    ) -> Self {
        Self {
            number: number.into(),
            kind: kind.into(),
            user_id,
            issued_at,
            customer_details: customer_details.map(|v| v.map(|v| v.into()).collect()),
            coins,
            net_total_cents,
            vat_total_cents,
            gross_total_cents,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct DocumentQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<DocumentBorrowed, tokio_postgres::Error>,
    mapper: fn(DocumentBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> DocumentQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(DocumentBorrowed) -> R) -> DocumentQuery<'c, 'a, 's, C, R, N> {
        DocumentQuery {
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
pub struct GetDocumentStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_document() -> GetDocumentStmt {
    GetDocumentStmt("select * from financial_documents where number=$1", None)
}
impl GetDocumentStmt {
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
        number: &'a T1,
    ) -> DocumentQuery<'c, 'a, 's, C, Document, 1> {
        DocumentQuery {
            client,
            params: [number],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<DocumentBorrowed, tokio_postgres::Error> {
                    Ok(DocumentBorrowed {
                        number: row.try_get(0)?,
                        kind: row.try_get(1)?,
                        user_id: row.try_get(2)?,
                        issued_at: row.try_get(3)?,
                        customer_details: row.try_get(4)?,
                        coins: row.try_get(5)?,
                        net_total_cents: row.try_get(6)?,
                        vat_total_cents: row.try_get(7)?,
                        gross_total_cents: row.try_get(8)?,
                    })
                },
            mapper: |it| Document::from(it),
        }
    }
}
pub struct RecordDocumentStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn record_document() -> RecordDocumentStmt {
    RecordDocumentStmt(
        "insert into financial_documents (number, kind, user_id, issued_at, customer_details, coins, net_total_cents, vat_total_cents, gross_total_cents) values ($1, $2, $3, $4, $5, $6, $7, $8, $9) on conflict (number) do update set user_id=coalesce(financial_documents.user_id, excluded.user_id), customer_details=coalesce(financial_documents.customer_details, excluded.customer_details), coins=coalesce(financial_documents.coins, excluded.coins), net_total_cents=coalesce(financial_documents.net_total_cents, excluded.net_total_cents), vat_total_cents=coalesce(financial_documents.vat_total_cents, excluded.vat_total_cents), gross_total_cents=coalesce(financial_documents.gross_total_cents, excluded.gross_total_cents)",
        None,
    )
}
impl RecordDocumentStmt {
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
        number: &'a T1,
        kind: &'a T2,
        user_id: &'a Option<uuid::Uuid>,
        issued_at: &'a chrono::DateTime<chrono::FixedOffset>,
        customer_details: &'a Option<T4>,
        coins: &'a Option<i64>,
        net_total_cents: &'a Option<i64>,
        vat_total_cents: &'a Option<i64>,
        gross_total_cents: &'a Option<i64>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    number,
                    kind,
                    user_id,
                    issued_at,
                    customer_details,
                    coins,
                    net_total_cents,
                    vat_total_cents,
                    gross_total_cents,
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
    T4: crate::ArraySql<Item = T3>,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        RecordDocumentParams<T1, T2, T3, T4>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for RecordDocumentStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a RecordDocumentParams<T1, T2, T3, T4>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.number,
            &params.kind,
            &params.user_id,
            &params.issued_at,
            &params.customer_details,
            &params.coins,
            &params.net_total_cents,
            &params.vat_total_cents,
            &params.gross_total_cents,
        ))
    }
}
pub struct PseudonymizeDocumentsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn pseudonymize_documents() -> PseudonymizeDocumentsStmt {
    PseudonymizeDocumentsStmt(
        "update financial_documents set customer_details=$1 where user_id=$2",
        None,
    )
}
impl PseudonymizeDocumentsStmt {
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
        T2: crate::ArraySql<Item = T1>,
    >(
        &'s self,
        client: &'c C,
        customer_details: &'a T2,
        user_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[customer_details, user_id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql, T2: crate::ArraySql<Item = T1>>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        PseudonymizeDocumentsParams<T1, T2>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for PseudonymizeDocumentsStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a PseudonymizeDocumentsParams<T1, T2>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.customer_details, &params.user_id))
    }
}
pub struct ListDocumentsIssuedBeforeStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_documents_issued_before() -> ListDocumentsIssuedBeforeStmt {
    ListDocumentsIssuedBeforeStmt(
        "select * from financial_documents where issued_at<$1 order by issued_at asc",
        None,
    )
}
impl ListDocumentsIssuedBeforeStmt {
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
        issued_before: &'a chrono::DateTime<chrono::FixedOffset>,
    ) -> DocumentQuery<'c, 'a, 's, C, Document, 1> {
        DocumentQuery {
            client,
            params: [issued_before],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<DocumentBorrowed, tokio_postgres::Error> {
                    Ok(DocumentBorrowed {
                        number: row.try_get(0)?,
                        kind: row.try_get(1)?,
                        user_id: row.try_get(2)?,
                        issued_at: row.try_get(3)?,
                        customer_details: row.try_get(4)?,
                        coins: row.try_get(5)?,
                        net_total_cents: row.try_get(6)?,
                        vat_total_cents: row.try_get(7)?,
                        gross_total_cents: row.try_get(8)?,
                    })
                },
            mapper: |it| Document::from(it),
        }
    }
}
pub struct DeleteDocumentsIssuedBeforeStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_documents_issued_before() -> DeleteDocumentsIssuedBeforeStmt {
    DeleteDocumentsIssuedBeforeStmt("delete from financial_documents where issued_at<$1", None)
}
impl DeleteDocumentsIssuedBeforeStmt {
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
        issued_before: &'a chrono::DateTime<chrono::FixedOffset>,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[issued_before]).await
    }
}
