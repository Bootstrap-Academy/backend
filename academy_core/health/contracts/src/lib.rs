use std::future::Future;

pub trait HealthFeatureService: Send + Sync + 'static {
    /// Return the current health status.
    fn get_status(&self) -> impl Future<Output = HealthStatus> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthStatus {
    pub database: bool,
    pub cache: bool,
    pub email: bool,
}
