use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_coin_contracts::coin::{CoinAddCoinsError, CoinService};
use academy_core_course_contracts::{CourseFeatureService, CoursePurchaseError};
use academy_data::course::CourseDataRepository;
use academy_di::Build;
use academy_email_contracts::template::TemplateEmailService;
use academy_models::{
    auth::AccessToken,
    course::{CourseFilter, CourseId, CourseUserPatchRef, CourseUserSummary},
};
use academy_persistence_contracts::{
    Database, Transaction, course::CourseRepository, user::UserRepository,
};
use academy_templates_contracts::CoursePurchaseConfirmationTemplate;
use academy_utils::{patch::PatchValue, trace_instrument};
use anyhow::{Context, anyhow};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Default, Build)]
pub struct CourseFeatureServiceImpl<Db, Auth, Coin, TemplateEmail, UserRepo, CourseRepo> {
    db: Db,
    auth: Auth,
    coin: Coin,
    template_email: TemplateEmail,
    user_repo: UserRepo,
    course_repo: CourseRepo,
    course_data_repo: CourseDataRepository,
}

impl<Db, Auth, Coin, TemplateEmail, UserRepo, CourseRepo> CourseFeatureService
    for CourseFeatureServiceImpl<Db, Auth, Coin, TemplateEmail, UserRepo, CourseRepo>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Coin: CoinService<Db::Transaction>,
    TemplateEmail: TemplateEmailService,
    UserRepo: UserRepository<Db::Transaction>,
    CourseRepo: CourseRepository<Db::Transaction>,
{
    #[trace_instrument(no_ret, skip(self))]
    async fn list(&self, filter: CourseFilter) -> anyhow::Result<Vec<CourseUserSummary>> {
        let mut courses = self
            .course_data_repo
            .values()
            .filter(|&course| {
                filter.search_term.as_ref().is_none_or(|search_term| {
                    course
                        .base
                        .title
                        .to_lowercase()
                        .contains(&search_term.to_lowercase())
                }) && filter.author.as_ref().is_none_or(|author| {
                    course
                        .base
                        .authors
                        .iter()
                        .any(|a| a.name.to_lowercase().contains(&author.to_lowercase()))
                }) && filter
                    .free
                    .is_none_or(|free| (course.base.price == 0) == free)
            })
            .cloned()
            .map(Into::into)
            .collect::<Vec<CourseUserSummary>>();

        if filter.search_term.is_some() {
            courses.sort_unstable_by_key(|c| c.base.title.len());
        } else {
            courses.sort_unstable_by(|a, b| a.base.title.cmp(&b.base.title));
        }

        Ok(courses)
    }

    #[trace_instrument(skip(self))]
    async fn purchase(
        &self,
        token: &AccessToken,
        course_id: CourseId,
    ) -> Result<(), CoursePurchaseError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        auth.ensure_email_verified().map_auth_err()?;

        let course = self
            .course_data_repo
            .get(&course_id)
            .ok_or(CoursePurchaseError::CourseNotFound)?;

        if course.base.price == 0 {
            return Err(CoursePurchaseError::CourseIsFree);
        }

        let mut txn = self.db.begin_transaction().await?;

        let course_user = self
            .course_repo
            .get_course_user(&mut txn, &course_id, auth.user_id)
            .await?;
        if course_user.purchased {
            return Err(CoursePurchaseError::AlreadyPurchased);
        }

        let transaction_description = format!("Course \"{}\"", *course.base.title)
            .try_into()
            .map_err(anyhow::Error::from)?;
        self.coin
            .add_coins(
                &mut txn,
                auth.user_id,
                -(course.base.price as i64),
                false,
                Some(transaction_description),
                false,
            )
            .await
            .map_err(|err| match err {
                CoinAddCoinsError::NotEnoughCoins => CoursePurchaseError::NotEnoughCoins,
                CoinAddCoinsError::Other(err) => err.into(),
            })?;

        self.course_repo
            .update_course_user(
                &mut txn,
                &course_id,
                auth.user_id,
                CourseUserPatchRef {
                    purchased: PatchValue::Update(&true),
                },
            )
            .await?;

        let user_composite = self
            .user_repo
            .get_composite(&mut txn, auth.user_id)
            .await?
            .ok_or_else(|| anyhow!("Failed to fetch authenticated user"))?;
        if let Some(email) = user_composite.user.email {
            self.template_email
                .send_course_purchase_confirmation_email(
                    email.with_name(user_composite.profile.display_name.into_inner()),
                    &CoursePurchaseConfirmationTemplate {
                        title: course.base.title.clone().into_inner(),
                    },
                )
                .await
                .context("Failed to send course purchase confirmation email")?;
        }

        txn.commit().await?;

        Ok(())
    }
}
