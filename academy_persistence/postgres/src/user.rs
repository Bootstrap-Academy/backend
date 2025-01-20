use std::str::FromStr;

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
use academy_utils::trace_instrument;
use bb8_postgres::tokio_postgres;
use clorinde::{
    client::Params,
    queries::{
        self,
        user::{
            CountCompositesParams, CreateInvoiceInfoParams, CreateParams, CreateProfileParams,
            GetCompositeByOauth2ProviderIdAndRemoteUserIdParams, ListCompositesParams,
            UpdateInvoiceInfoParams, UpdateParams, UpdateProfileParams,
        },
    },
};
use futures::{StreamExt, TryStreamExt};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Copy, Default, Build)]
pub struct PostgresUserRepository;

impl UserRepository<PostgresTransaction> for PostgresUserRepository {
    #[trace_instrument(skip(self, txn))]
    async fn count(
        &self,
        txn: &mut PostgresTransaction,
        filter: &UserFilter,
    ) -> anyhow::Result<u64> {
        let params = CountCompositesParams {
            name: filter.name.as_deref(),
            email: filter.email.as_deref(),
            enabled: filter.enabled,
            admin: filter.admin,
            mfa_enabled: filter.mfa_enabled,
            email_verified: filter.email_verified,
            newsletter: filter.newsletter,
        };

        queries::user::count_composites()
            .params(txn.txn(), &params)
            .one()
            .await
            .map_err(Into::into)
            .and_then(|row| row.try_into().map_err(Into::into))
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_composites(
        &self,
        txn: &mut PostgresTransaction,
        filter: &UserFilter,
        pagination: PaginationSlice,
    ) -> anyhow::Result<Vec<UserComposite>> {
        let params = ListCompositesParams {
            name: filter.name.as_deref(),
            email: filter.email.as_deref(),
            enabled: filter.enabled,
            admin: filter.admin,
            mfa_enabled: filter.mfa_enabled,
            email_verified: filter.email_verified,
            newsletter: filter.newsletter,
            limit: (*pagination.limit).try_into()?,
            offset: pagination.offset.try_into()?,
        };

        queries::user::list_composites()
            .params(txn.txn(), &params)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_composite))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn exists(&self, txn: &mut PostgresTransaction, user_id: UserId) -> anyhow::Result<bool> {
        queries::user::exists()
            .bind(txn.txn(), &user_id)
            .one()
            .await
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_composite(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<UserComposite>> {
        queries::user::get_composite()
            .bind(txn.txn(), &user_id)
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| row.map(decode_composite).transpose())
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_composite_by_name(
        &self,
        txn: &mut PostgresTransaction,
        name: &UserName,
    ) -> anyhow::Result<Option<UserComposite>> {
        queries::user::get_composite_by_name()
            .bind(txn.txn(), &**name)
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| row.map(decode_composite).transpose())
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_composite_by_email(
        &self,
        txn: &mut PostgresTransaction,
        email: &EmailAddress,
    ) -> anyhow::Result<Option<UserComposite>> {
        queries::user::get_composite_by_email()
            .bind(txn.txn(), &email.as_str())
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| row.map(decode_composite).transpose())
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_composite_by_oauth2_provider_id_and_remote_user_id(
        &self,
        txn: &mut PostgresTransaction,
        provider_id: &OAuth2ProviderId,
        remote_user_id: &OAuth2RemoteUserId,
    ) -> anyhow::Result<Option<UserComposite>> {
        let params = GetCompositeByOauth2ProviderIdAndRemoteUserIdParams {
            provider_id: &**provider_id,
            remote_user_id: &**remote_user_id,
        };

        queries::user::get_composite_by_oauth2_provider_id_and_remote_user_id()
            .params(txn.txn(), &params)
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| row.map(decode_composite).transpose())
    }

    #[trace_instrument(skip(self, txn))]
    async fn create(
        &self,
        txn: &mut PostgresTransaction,
        user: &User,
        profile: &UserProfile,
        invoice_info: &UserInvoiceInfo,
    ) -> Result<(), UserRepoError> {
        let user_params = CreateParams {
            id: *user.id,
            name: &*user.name,
            email: user.email.as_ref().map(EmailAddress::as_str),
            email_verified: user.email_verified,
            created_at: user.created_at.into(),
            last_login: user.last_login.map(Into::into),
            last_name_change: user.last_name_change.map(Into::into),
            enabled: user.enabled,
            admin: user.admin,
            newsletter: user.newsletter,
        };

        let profile_params = CreateProfileParams {
            user_id: *user.id,
            display_name: &*profile.display_name,
            bio: &*profile.bio,
            tags: profile.tags.iter().map(|tag| &**tag).collect::<Vec<_>>(),
        };

        let invoice_info_params = CreateInvoiceInfoParams {
            user_id: *user.id,
            business: invoice_info.business,
            first_name: invoice_info.first_name.as_deref(),
            last_name: invoice_info.last_name.as_deref(),
            street: invoice_info.street.as_deref(),
            zip_code: invoice_info.zip_code.as_deref(),
            city: invoice_info.city.as_deref(),
            country: invoice_info.country.as_deref(),
            vat_id: invoice_info.vat_id.as_deref(),
        };

        queries::user::create()
            .params(txn.txn(), &user_params)
            .await
            .map_err(map_user_repo_error)?;

        queries::user::create_profile()
            .params(txn.txn(), &profile_params)
            .await
            .map_err(anyhow::Error::from)?;

        queries::user::create_invoice_info()
            .params(txn.txn(), &invoice_info_params)
            .await
            .map_err(anyhow::Error::from)?;

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
        let params = UpdateParams {
            id: *user_id,
            name: name.update().map(|x| &**x),
            email: email
                .update()
                .and_then(Option::as_ref)
                .map(EmailAddress::as_str),
            email_verified: email_verified.update().copied(),
            last_login: last_login.update().copied().flatten().map(Into::into),
            last_name_change: last_name_change.update().copied().flatten().map(Into::into),
            enabled: enabled.update().copied(),
            admin: admin.update().copied(),
            newsletter: newsletter.update().copied(),
        };

        queries::user::update()
            .params(txn.txn(), &params)
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
        let params = UpdateProfileParams {
            user_id: *user_id,
            display_name: display_name.update().map(|x| &**x),
            bio: bio.update().map(|x| &**x),
            tags: tags
                .update()
                .map(|tags| tags.iter().map(|x| &**x).collect::<Vec<_>>()),
        };

        queries::user::update_profile()
            .params(txn.txn(), &params)
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
        let params = UpdateInvoiceInfoParams {
            user_id: *user_id,
            clear_business: business.is_update_and(|x| x.is_none()),
            business: business.update().and_then(|x| x.as_ref().copied()),
            clear_first_name: first_name.is_update_and(|x| x.is_none()),
            first_name: first_name.update().and_then(Option::as_deref),
            clear_last_name: last_name.is_update_and(|x| x.is_none()),
            last_name: last_name.update().and_then(Option::as_deref),
            clear_street: street.is_update_and(|x| x.is_none()),
            street: street.update().and_then(Option::as_deref),
            clear_zip_code: zip_code.is_update_and(|x| x.is_none()),
            zip_code: zip_code.update().and_then(Option::as_deref),
            clear_city: city.is_update_and(|x| x.is_none()),
            city: city.update().and_then(Option::as_deref),
            clear_country: country.is_update_and(|x| x.is_none()),
            country: country.update().and_then(Option::as_deref),
            clear_vat_id: vat_id.is_update_and(|x| x.is_none()),
            vat_id: vat_id.update().and_then(Option::as_deref),
        };

        queries::user::update_invoice_info()
            .params(txn.txn(), &params)
            .await
            .map(|n| n != 0)
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn delete(&self, txn: &mut PostgresTransaction, user_id: UserId) -> anyhow::Result<bool> {
        queries::user::delete()
            .bind(txn.txn(), &user_id)
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
        queries::user::set_password_hash()
            .bind(txn.txn(), &user_id, &password_hash)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_password_hash(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<String>> {
        queries::user::get_password_hash()
            .bind(txn.txn(), &user_id)
            .opt()
            .await
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn remove_password_hash(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<bool> {
        queries::user::remove_password_hash()
            .bind(txn.txn(), &user_id)
            .await
            .map(|n| n != 0)
            .map_err(Into::into)
    }

    async fn get_number(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<u64> {
        queries::user::get_number()
            .bind(txn.txn(), &user_id)
            .one()
            .await
            .map_err(Into::into)
            .and_then(|row| row.try_into().map_err(Into::into))
    }
}

fn decode_composite(value: queries::user::UserComposite) -> anyhow::Result<UserComposite> {
    let user = User {
        id: value.id.into(),
        name: value.name.try_into()?,
        email: value.email.as_deref().map(FromStr::from_str).transpose()?,
        email_verified: value.email_verified,
        created_at: value.created_at.into(),
        last_login: value.last_login.map(Into::into),
        last_name_change: value.last_name_change.map(Into::into),
        enabled: value.enabled,
        admin: value.admin,
        newsletter: value.newsletter,
    };

    let profile = UserProfile {
        display_name: value.display_name.try_into()?,
        bio: value.bio.try_into()?,
        tags: value
            .tags
            .into_iter()
            .map(|tag| tag.try_into())
            .collect::<Result<Vec<_>, _>>()?
            .try_into()?,
    };

    let details = UserDetails {
        mfa_enabled: value.mfa_enabled,
        password_login: value.password_login,
        oauth2_login: value.oauth2_login,
    };

    let invoice_info = UserInvoiceInfo {
        business: value.business,
        first_name: value.first_name.map(TryInto::try_into).transpose()?,
        last_name: value.last_name.map(TryInto::try_into).transpose()?,
        street: value.street.map(TryInto::try_into).transpose()?,
        zip_code: value.zip_code.map(TryInto::try_into).transpose()?,
        city: value.city.map(TryInto::try_into).transpose()?,
        country: value.country.map(TryInto::try_into).transpose()?,
        vat_id: value.vat_id.map(TryInto::try_into).transpose()?,
    };

    Ok(UserComposite {
        user,
        profile,
        details,
        invoice_info,
    })
}

fn map_user_repo_error(err: tokio_postgres::Error) -> UserRepoError {
    match err.as_db_error() {
        Some(err) if err.constraint() == Some("users_name_idx") => UserRepoError::NameConflict,
        Some(err) if err.constraint() == Some("users_email_idx") => UserRepoError::EmailConflict,
        _ => UserRepoError::Other(err.into()),
    }
}
