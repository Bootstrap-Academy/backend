use std::fmt::Debug;

use academy_di::Build;
use academy_models::Sha256Hash;
use academy_shared_contracts::hash::HashService;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, Build)]
pub struct HashServiceImpl;

impl HashService for HashServiceImpl {
    #[tracing::instrument(skip(self, data))]
    fn sha256<T: AsRef<[u8]> + Debug>(&self, data: &T) -> Sha256Hash {
        Sha256Hash(Sha256::new().chain_update(data).finalize().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256() {
        // Arrange
        let data = b"hello world";
        let expected = Sha256Hash(
            hex::decode("b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9")
                .unwrap()
                .try_into()
                .unwrap(),
        );

        let sut = HashServiceImpl;

        // Act
        let result = sut.sha256(data);

        // Assert
        assert_eq!(result, expected);
    }
}
