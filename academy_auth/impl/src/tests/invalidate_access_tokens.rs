use academy_auth_contracts::{access_token::MockAuthAccessTokenService, AuthService};
use academy_demo::{user::FOO, SHA256HASH1, SHA256HASH2};
use academy_models::session::SessionRefreshTokenHash;
use academy_persistence_contracts::session::MockSessionRepository;

use crate::{tests::Sut, AuthServiceImpl};

#[tokio::test]
async fn ok() {
    // Arrange
    let hashes = [
        SessionRefreshTokenHash::from(*SHA256HASH1),
        SessionRefreshTokenHash::from(*SHA256HASH2),
    ];

    let session_repo = MockSessionRepository::new()
        .with_list_refresh_token_hashes_by_user(FOO.user.id, hashes.into());

    let auth_access_token = MockAuthAccessTokenService::new()
        .with_invalidate(hashes[0])
        .with_invalidate(hashes[1]);

    let sut = AuthServiceImpl {
        session_repo,
        auth_access_token,
        ..Sut::default()
    };

    // Act
    let result = sut.invalidate_access_tokens(&mut (), FOO.user.id).await;

    // Assert
    result.unwrap();
}
