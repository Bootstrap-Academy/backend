use academy_di::Build;
use academy_models::{Sensitive, VerificationCode, mfa::MfaRecoveryCode};
use academy_shared_contracts::secret::SecretService;
use academy_utils::trace_instrument;
use rand::{
    CryptoRng, Rng, RngCore,
    distr::{Alphanumeric, Distribution, SampleString, Uniform},
    rng,
};

#[derive(Debug, Clone, Copy, Build)]
pub struct SecretServiceImpl;

impl SecretService for SecretServiceImpl {
    #[trace_instrument(skip(self))]
    fn generate(&self, len: usize) -> Sensitive<String> {
        Alphanumeric.sample_string(&mut csprng(), len).into()
    }

    #[trace_instrument(skip(self))]
    fn generate_bytes(&self, len: usize) -> Sensitive<Vec<u8>> {
        let mut out = vec![0; len];
        csprng().fill_bytes(&mut out);
        out.into()
    }

    #[trace_instrument(skip(self))]
    fn generate_verification_code(&self) -> VerificationCode {
        generate_hyphenated_code(
            csprng(),
            uppercase_digits(),
            VerificationCode::CHUNK_COUNT,
            VerificationCode::CHUNK_SIZE,
        )
        .try_into()
        .unwrap()
    }

    #[trace_instrument(skip(self))]
    fn generate_mfa_recovery_code(&self) -> MfaRecoveryCode {
        generate_hyphenated_code(
            csprng(),
            uppercase_digits(),
            MfaRecoveryCode::CHUNK_COUNT,
            MfaRecoveryCode::CHUNK_SIZE,
        )
        .try_into()
        .unwrap()
    }
}

fn generate_hyphenated_code(
    rng: impl Rng,
    dist: impl Distribution<char>,
    chunk_count: usize,
    chunk_size: usize,
) -> String {
    let mut chars = dist.sample_iter(rng);

    let len = chunk_count * (chunk_size + 1) - 1;
    let mut out = String::with_capacity(len);

    out.extend(chars.by_ref().take(chunk_size));
    for _ in 1..chunk_count {
        out.push('-');
        out.extend(chars.by_ref().take(chunk_size));
    }

    debug_assert_eq!(out.len(), len);
    out
}

fn csprng() -> impl Rng + CryptoRng {
    rng()
}

fn uppercase_digits() -> impl Distribution<char> {
    Uniform::new(0u8, 10 + 26)
        .unwrap()
        .map(|x| (x + b'0' + (x >= 10) as u8 * 7) as char)
}

#[cfg(test)]
mod tests {
    use rand::rngs::mock::StepRng;

    use super::*;

    #[test]
    fn generate() {
        // Arrange
        let n = 4096;

        let sut = SecretServiceImpl;

        // Act
        let result = sut.generate(n).0;

        // Assert
        assert_eq!(result.len(), n);
        assert!(
            result
                .chars()
                .all(|c| matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9'))
        );
    }

    #[test]
    fn generate_bytes() {
        // Arrange
        let n = 16;

        let sut = SecretServiceImpl;

        // Act
        let a = sut.generate_bytes(n);
        let b = sut.generate_bytes(n);

        // Assert
        assert_ne!(a, b);
    }

    #[test]
    fn generate_verification_code() {
        // Arrange
        let sut = SecretServiceImpl;

        // Act + Assert
        for _ in 0..4096 {
            sut.generate_verification_code();
        }
    }

    #[test]
    fn generate_mfa_recovery_code() {
        // Arrange
        let sut = SecretServiceImpl;

        // Act + Assert
        for _ in 0..4096 {
            sut.generate_mfa_recovery_code();
        }
    }

    #[test]
    fn uppercase_digits() {
        // Arrange
        let expected = ('0'..='9').chain('A'..='Z').collect::<String>();
        let rng = StepRng::new(0, (1 << 32) / expected.len() as u64);
        let dist = super::uppercase_digits();

        // Act
        let result = dist
            .sample_iter(rng)
            .take(expected.len())
            .collect::<String>();

        // Assert
        assert_eq!(result, expected);
    }
}
