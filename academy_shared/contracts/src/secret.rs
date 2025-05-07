use academy_models::{Sensitive, VerificationCode, mfa::MfaRecoveryCode};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait SecretService: Send + Sync + 'static {
    /// Generate a new random alphanumeric string of the given length.
    fn generate(&self, len: usize) -> Sensitive<String>;

    /// Generate `len` bytes of random data.
    fn generate_bytes(&self, len: usize) -> Sensitive<Vec<u8>>;

    /// Generate a new random verification code.
    fn generate_verification_code(&self) -> VerificationCode;

    /// Generate a new random mfa recovery code.
    fn generate_mfa_recovery_code(&self) -> MfaRecoveryCode;
}

#[cfg(feature = "mock")]
impl MockSecretService {
    pub fn with_generate(mut self, len: usize, result: String) -> Self {
        self.expect_generate()
            .once()
            .with(mockall::predicate::eq(len))
            .return_once(|_| result.into());
        self
    }

    pub fn with_generate_bytes(mut self, len: usize, result: Vec<u8>) -> Self {
        self.expect_generate_bytes()
            .once()
            .with(mockall::predicate::eq(len))
            .return_once(|_| result.into());
        self
    }

    pub fn with_generate_verification_code(mut self, result: VerificationCode) -> Self {
        self.expect_generate_verification_code()
            .once()
            .with()
            .return_once(|| result);
        self
    }

    pub fn with_generate_mfa_recovery_code(mut self, result: MfaRecoveryCode) -> Self {
        self.expect_generate_mfa_recovery_code()
            .once()
            .with()
            .return_once(|| result);
        self
    }
}
