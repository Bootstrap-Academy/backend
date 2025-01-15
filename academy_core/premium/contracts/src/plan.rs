use academy_models::premium::{PremiumPlan, PremiumPlanDetails};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait PremiumPlanService: Send + Sync + 'static {
    /// Return the details for the given premium plan.
    fn get_details(&self, plan: PremiumPlan) -> PremiumPlanDetails;
}

#[cfg(feature = "mock")]
impl MockPremiumPlanService {
    pub fn with_get_details(mut self, plan: PremiumPlan, result: PremiumPlanDetails) -> Self {
        self.expect_get_details()
            .once()
            .with(mockall::predicate::eq(plan))
            .return_once(move |_| result);
        self
    }
}
