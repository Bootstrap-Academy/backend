use std::fmt::Write;

use academy_di::Build;
use academy_models::{
    email_address::EmailAddress,
    oauth2::{OAuth2ProviderId, OAuth2RemoteUserId},
    pagination::PaginationSlice,
    user::{
        User, UserComposite, UserDetails, UserFilter, UserId, UserInvoiceInfo,
        UserInvoiceInfoPatchRef, UserName, UserPatchRef, UserProfile, UserProfilePatchRef,
    },
};
use academy_persistence_contracts::user::{UserRepoError, UserRepository};
use academy_utils::{patch::PatchValue, trace_instrument};
use bb8_postgres::tokio_postgres::{self, types::ToSql, Row};
use uuid::Uuid;

use crate::{arg_indices, columns, ColumnCounter, PostgresTransaction};

#[derive(Debug, Clone, Copy, Default, Build)]
pub struct PostgresUserRepository;

columns!(user as "u": "id", "name", "email", "email_verified", "created_at", "last_login", "last_name_change", "enabled", "admin", "newsletter");
columns!(profile as "p": "user_id", "display_name", "bio", "tags");
columns!(details as "d": "user_id", "mfa_enabled", "password_login", "oauth2_login");
columns!(invoice_info as "i": "user_id", "business", "first_name", "last_name", "street", "zip_code", "city", "country", "vat_id");

const JOIN_PROFILE: &str = "inner join user_profiles p on u.id=p.user_id";
const JOIN_DETAILS: &str = "inner join user_details d on u.id=d.user_id";
const JOIN_INVOICE_INFO: &str = "inner join user_invoice_info i on u.id=i.user_id";

impl UserRepository<PostgresTransaction> for PostgresUserRepository {
    #[trace_instrument(skip(self, txn))]
    async fn count(
        &self,
        txn: &mut PostgresTransaction,
        filter: &UserFilter,
    ) -> anyhow::Result<u64> {
        let mut query = "select count(*) from users u ".to_owned();
        if filter.name.is_some() {
            query.push_str(JOIN_PROFILE)
        }
        if filter.mfa_enabled.is_some() {
            query.push_str(JOIN_DETAILS)
        }
        query.push_str(" where true");

        let mut params: Vec<&(dyn ToSql + Sync)> = Vec::new();
        make_filter(filter, &mut query, &mut params);

        txn.txn()
            .query_one(&query, &params)
            .await
            .map(|row| row.get::<_, i64>(0) as _)
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_composites(
        &self,
        txn: &mut PostgresTransaction,
        filter: &UserFilter,
        pagination: PaginationSlice,
    ) -> anyhow::Result<Vec<UserComposite>> {
        let mut query = format!(
            "select {USER_COLS}, {PROFILE_COLS}, {DETAILS_COLS}, {INVOICE_INFO_COLS} from users u \
             {JOIN_PROFILE} {JOIN_DETAILS} {JOIN_INVOICE_INFO} where true"
        );
        let mut params: Vec<&(dyn ToSql + Sync)> = Vec::new();
        make_filter(filter, &mut query, &mut params);
        query.push_str(&format!(
            " order by u.created_at asc limit {} offset {}",
            *pagination.limit, pagination.offset
        ));

        txn.txn()
            .query(&query, &params)
            .await
            .map_err(Into::into)
            .and_then(|rows| {
                rows.into_iter()
                    .map(|row| decode_composite(&row, &mut Default::default()))
                    .collect()
            })
    }

    #[trace_instrument(skip(self, txn))]
    async fn exists(&self, txn: &mut PostgresTransaction, user_id: UserId) -> anyhow::Result<bool> {
        txn.txn()
            .query_opt("select id from users where id=$1", &[&*user_id])
            .await
            .map(|row| row.is_some())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_composite(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<UserComposite>> {
        txn.txn()
            .query_opt(
                &format!(
                    "select {USER_COLS}, {PROFILE_COLS}, {DETAILS_COLS}, {INVOICE_INFO_COLS} from \
                     users u {JOIN_PROFILE} {JOIN_DETAILS} {JOIN_INVOICE_INFO} where id=$1"
                ),
                &[&*user_id],
            )
            .await
            .map_err(Into::into)
            .and_then(|row| {
                row.map(|row| decode_composite(&row, &mut Default::default()))
                    .transpose()
            })
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_composite_by_name(
        &self,
        txn: &mut PostgresTransaction,
        name: &UserName,
    ) -> anyhow::Result<Option<UserComposite>> {
        txn.txn()
            .query_opt(
                &format!(
                    "select {USER_COLS}, {PROFILE_COLS}, {DETAILS_COLS}, {INVOICE_INFO_COLS} from \
                     users u {JOIN_PROFILE} {JOIN_DETAILS} {JOIN_INVOICE_INFO} where \
                     lower(name)=lower($1)"
                ),
                &[&name.as_str()],
            )
            .await
            .map_err(Into::into)
            .and_then(|row| {
                row.map(|row| decode_composite(&row, &mut Default::default()))
                    .transpose()
            })
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_composite_by_email(
        &self,
        txn: &mut PostgresTransaction,
        email: &EmailAddress,
    ) -> anyhow::Result<Option<UserComposite>> {
        txn.txn()
            .query_opt(
                &format!(
                    "select {USER_COLS}, {PROFILE_COLS}, {DETAILS_COLS}, {INVOICE_INFO_COLS} from \
                     users u {JOIN_PROFILE} {JOIN_DETAILS} {JOIN_INVOICE_INFO} where \
                     lower(email)=lower($1)"
                ),
                &[&email.as_str()],
            )
            .await
            .map_err(Into::into)
            .and_then(|row| {
                row.map(|row| decode_composite(&row, &mut Default::default()))
                    .transpose()
            })
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_composite_by_oauth2_provider_id_and_remote_user_id(
        &self,
        txn: &mut PostgresTransaction,
        provider_id: &OAuth2ProviderId,
        remote_user_id: &OAuth2RemoteUserId,
    ) -> anyhow::Result<Option<UserComposite>> {
        txn.txn()
            .query_opt(
                &format!(
                    "select {USER_COLS}, {PROFILE_COLS}, {DETAILS_COLS}, {INVOICE_INFO_COLS} from \
                     users u {JOIN_PROFILE} {JOIN_DETAILS} {JOIN_INVOICE_INFO} inner join \
                     oauth2_links ol on u.id=ol.user_id where ol.provider_id=$1 and \
                     ol.remote_user_id=$2"
                ),
                &[&**provider_id, &**remote_user_id],
            )
            .await
            .map_err(Into::into)
            .and_then(|row| {
                row.map(|row| decode_composite(&row, &mut Default::default()))
                    .transpose()
            })
    }

    #[trace_instrument(skip(self, txn))]
    async fn create(
        &self,
        txn: &mut PostgresTransaction,
        user: &User,
        profile: &UserProfile,
        invoice_info: &UserInvoiceInfo,
    ) -> Result<(), UserRepoError> {
        txn.txn()
            .execute(
                &format!(
                    "insert into users ({USER_COL_NAMES}) values ({})",
                    arg_indices(1..=USER_CNT)
                ),
                &[
                    &*user.id,
                    &user.name.as_str(),
                    &user.email.as_ref().map(EmailAddress::as_str),
                    &user.email_verified,
                    &user.created_at,
                    &user.last_login,
                    &user.last_name_change,
                    &user.enabled,
                    &user.admin,
                    &user.newsletter,
                ],
            )
            .await
            .map_err(map_user_repo_error)?;

        txn.txn()
            .execute(
                &format!(
                    "insert into user_profiles ({PROFILE_COL_NAMES}) values ({})",
                    arg_indices(1..=PROFILE_CNT)
                ),
                &[
                    &*user.id,
                    &profile.display_name.as_str(),
                    &profile.bio.as_str(),
                    &profile.tags.iter().map(|x| x.as_str()).collect::<Vec<_>>(),
                ],
            )
            .await
            .map_err(|err| UserRepoError::Other(err.into()))?;

        txn.txn()
            .execute(
                &format!(
                    "insert into user_invoice_info ({INVOICE_INFO_COL_NAMES}) values ({})",
                    arg_indices(1..=INVOICE_INFO_CNT)
                ),
                &[
                    &*user.id,
                    &invoice_info.business,
                    &invoice_info.first_name.as_deref(),
                    &invoice_info.last_name.as_deref(),
                    &invoice_info.street.as_deref(),
                    &invoice_info.zip_code.as_deref(),
                    &invoice_info.city.as_deref(),
                    &invoice_info.country.as_deref(),
                    &invoice_info.vat_id.as_deref(),
                ],
            )
            .await
            .map_err(|err| UserRepoError::Other(err.into()))?;

        Ok(())
    }

    #[trace_instrument(skip(self, txn))]
    async fn update<'a>(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        UserPatchRef {
            name,
            email,
            email_verified,
            last_login,
            last_name_change,
            enabled,
            admin,
            newsletter,
        }: UserPatchRef<'a>,
    ) -> Result<bool, UserRepoError> {
        let mut query = "update users set id=id".to_owned();
        let mut params: Vec<&(dyn ToSql + Sync)> = vec![&*user_id];

        let email = email.map(|x| x.as_ref().map(|x| x.as_str()));

        if let PatchValue::Update(name) = name {
            params.push(&**name);
            write!(&mut query, ", name=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(email) = &email {
            params.push(email);
            write!(&mut query, ", email=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(email_verified) = email_verified {
            params.push(email_verified);
            write!(&mut query, ", email_verified=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(last_login) = last_login {
            params.push(last_login);
            write!(&mut query, ", last_login=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(last_name_change) = last_name_change {
            params.push(last_name_change);
            write!(&mut query, ", last_name_change=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(enabled) = enabled {
            params.push(enabled);
            write!(&mut query, ", enabled=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(admin) = admin {
            params.push(admin);
            write!(&mut query, ", admin=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(newsletter) = newsletter {
            params.push(newsletter);
            write!(&mut query, ", newsletter=${}", params.len()).unwrap();
        }

        query.push_str(" where id=$1");

        txn.txn()
            .execute(&query, &params)
            .await
            .map(|n| n != 0)
            .map_err(map_user_repo_error)
    }

    #[trace_instrument(skip(self, txn))]
    async fn update_profile<'a>(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        UserProfilePatchRef {
            display_name,
            bio,
            tags,
        }: UserProfilePatchRef<'a>,
    ) -> anyhow::Result<bool> {
        let mut query = "update user_profiles set user_id=user_id".to_owned();
        let mut params: Vec<&(dyn ToSql + Sync)> = vec![&*user_id];

        let tags = tags.map(|x| x.iter().map(|x| x.as_str()).collect::<Vec<_>>());

        if let PatchValue::Update(display_name) = display_name {
            params.push(&**display_name);
            write!(&mut query, ", display_name=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(bio) = bio {
            params.push(&**bio);
            write!(&mut query, ", bio=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(tags) = &tags {
            params.push(tags);
            write!(&mut query, ", tags=${}", params.len()).unwrap();
        }

        query.push_str(" where user_id=$1");

        txn.txn()
            .execute(&query, &params)
            .await
            .map(|n| n != 0)
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn update_invoice_info<'a>(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        UserInvoiceInfoPatchRef {
            business,
            first_name,
            last_name,
            street,
            zip_code,
            city,
            country,
            vat_id,
        }: UserInvoiceInfoPatchRef<'a>,
    ) -> anyhow::Result<bool> {
        let mut query = "update user_invoice_info set user_id=user_id".to_owned();
        let mut params: Vec<&(dyn ToSql + Sync)> = vec![&*user_id];

        let first_name = first_name.map(|x| x.as_deref());
        let last_name = last_name.map(|x| x.as_deref());
        let street = street.map(|x| x.as_deref());
        let zip_code = zip_code.map(|x| x.as_deref());
        let city = city.map(|x| x.as_deref());
        let country = country.map(|x| x.as_deref());
        let vat_id = vat_id.map(|x| x.as_deref());

        if let PatchValue::Update(business) = &business {
            params.push(business);
            write!(&mut query, ", business=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(first_name) = &first_name {
            params.push(first_name);
            write!(&mut query, ", first_name=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(last_name) = &last_name {
            params.push(last_name);
            write!(&mut query, ", last_name=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(street) = &street {
            params.push(street);
            write!(&mut query, ", street=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(zip_code) = &zip_code {
            params.push(zip_code);
            write!(&mut query, ", zip_code=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(city) = &city {
            params.push(city);
            write!(&mut query, ", city=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(country) = &country {
            params.push(country);
            write!(&mut query, ", country=${}", params.len()).unwrap();
        }
        if let PatchValue::Update(vat_id) = &vat_id {
            params.push(vat_id);
            write!(&mut query, ", vat_id=${}", params.len()).unwrap();
        }

        query.push_str(" where user_id=$1");

        txn.txn()
            .execute(&query, &params)
            .await
            .map(|n| n != 0)
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn delete(&self, txn: &mut PostgresTransaction, user_id: UserId) -> anyhow::Result<bool> {
        txn.txn()
            .execute("delete from users where id=$1", &[&*user_id])
            .await
            .map(|x| x != 0)
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn save_password_hash(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        password_hash: String,
    ) -> anyhow::Result<()> {
        txn.txn()
            .execute(
                "insert into user_passwords (user_id, password_hash) values ($1, $2) on conflict \
                 (user_id) do update set password_hash=$2",
                &[&*user_id, &password_hash],
            )
            .await?;
        Ok(())
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_password_hash(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<String>> {
        txn.txn()
            .query_opt(
                "select password_hash from user_passwords where user_id=$1",
                &[&*user_id],
            )
            .await
            .map(|row| row.map(|row| row.get(0)))
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn remove_password_hash(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<bool> {
        txn.txn()
            .execute("delete from user_passwords where user_id=$1", &[&*user_id])
            .await
            .map(|n| n != 0)
            .map_err(Into::into)
    }

    async fn get_number(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<u64> {
        if let Some(number) = txn
            .txn()
            .query_opt(
                "select number from user_numbers where user_id=$1",
                &[&*user_id],
            )
            .await
            .map(|row| row.map(|row| row.get::<_, i64>(0) as _))?
        {
            return Ok(number);
        }

        txn.txn()
            .query_one(
                "insert into user_numbers as un (user_id, number) values ($1, \
                 nextval('user_number')) on conflict (user_id) do update set number=un.number \
                 returning number",
                &[&*user_id],
            )
            .await
            .map(|row| row.get::<_, i64>(0) as _)
            .map_err(Into::into)
    }
}

fn make_filter<'a>(
    filter: &'a UserFilter,
    query: &mut String,
    params: &mut Vec<&'a (dyn ToSql + Sync)>,
) {
    if let Some(name) = &filter.name {
        params.push(&**name);
        query.push_str(&format!(
            " and (lower(name)~lower(${0}) or lower(display_name)~lower(${0}))",
            params.len()
        ));
    }
    if let Some(email) = &filter.email {
        params.push(&**email);
        query.push_str(&format!(" and lower(email)~lower(${})", params.len()));
    }
    if let Some(enabled) = &filter.enabled {
        params.push(enabled);
        query.push_str(&format!(" and enabled=${}", params.len()));
    }
    if let Some(admin) = &filter.admin {
        params.push(admin);
        query.push_str(&format!(" and admin=${}", params.len()));
    }
    if let Some(mfa_enabled) = &filter.mfa_enabled {
        params.push(mfa_enabled);
        query.push_str(&format!(" and mfa_enabled=${}", params.len()));
    }
    if let Some(email_verified) = &filter.email_verified {
        params.push(email_verified);
        query.push_str(&format!(" and email_verified=${}", params.len()));
    }
    if let Some(newsletter) = &filter.newsletter {
        params.push(newsletter);
        query.push_str(&format!(" and newsletter=${}", params.len()));
    }
}

fn decode_user(row: &Row, cnt: &mut ColumnCounter) -> anyhow::Result<User> {
    Ok(User {
        id: row.get::<_, Uuid>(cnt.idx()).into(),
        name: row.get::<_, String>(cnt.idx()).try_into()?,
        email: row
            .get::<_, Option<String>>(cnt.idx())
            .as_deref()
            .map(str::parse)
            .transpose()?,
        email_verified: row.get(cnt.idx()),
        created_at: row.get(cnt.idx()),
        last_login: row.get(cnt.idx()),
        last_name_change: row.get(cnt.idx()),
        enabled: row.get(cnt.idx()),
        admin: row.get(cnt.idx()),
        newsletter: row.get(cnt.idx()),
    })
}

fn decode_profile(row: &Row, cnt: &mut ColumnCounter) -> anyhow::Result<UserProfile> {
    cnt.idx(); // user_id
    Ok(UserProfile {
        display_name: row.get::<_, String>(cnt.idx()).try_into()?,
        bio: row.get::<_, String>(cnt.idx()).try_into()?,
        tags: row
            .get::<_, Vec<String>>(cnt.idx())
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?
            .try_into()?,
    })
}

fn decode_details(row: &Row, cnt: &mut ColumnCounter) -> anyhow::Result<UserDetails> {
    cnt.idx(); // user_id
    Ok(UserDetails {
        mfa_enabled: row.get(cnt.idx()),
        password_login: row.get(cnt.idx()),
        oauth2_login: row.get(cnt.idx()),
    })
}

fn decode_invoice_info(row: &Row, cnt: &mut ColumnCounter) -> anyhow::Result<UserInvoiceInfo> {
    cnt.idx(); // user_id
    Ok(UserInvoiceInfo {
        business: row.get(cnt.idx()),
        first_name: row
            .get::<_, Option<String>>(cnt.idx())
            .map(TryInto::try_into)
            .transpose()?,
        last_name: row
            .get::<_, Option<String>>(cnt.idx())
            .map(TryInto::try_into)
            .transpose()?,
        street: row
            .get::<_, Option<String>>(cnt.idx())
            .map(TryInto::try_into)
            .transpose()?,
        zip_code: row
            .get::<_, Option<String>>(cnt.idx())
            .map(TryInto::try_into)
            .transpose()?,
        city: row
            .get::<_, Option<String>>(cnt.idx())
            .map(TryInto::try_into)
            .transpose()?,
        country: row
            .get::<_, Option<String>>(cnt.idx())
            .map(TryInto::try_into)
            .transpose()?,
        vat_id: row
            .get::<_, Option<String>>(cnt.idx())
            .map(TryInto::try_into)
            .transpose()?,
    })
}

fn decode_composite(row: &Row, cnt: &mut ColumnCounter) -> anyhow::Result<UserComposite> {
    Ok(UserComposite {
        user: decode_user(row, cnt)?,
        profile: decode_profile(row, cnt)?,
        details: decode_details(row, cnt)?,
        invoice_info: decode_invoice_info(row, cnt)?,
    })
}

fn map_user_repo_error(err: tokio_postgres::Error) -> UserRepoError {
    match err.as_db_error() {
        Some(err) if err.constraint() == Some("users_name_idx") => UserRepoError::NameConflict,
        Some(err) if err.constraint() == Some("users_email_idx") => UserRepoError::EmailConflict,
        _ => UserRepoError::Other(err.into()),
    }
}
