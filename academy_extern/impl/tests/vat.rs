use academy_di::{Provide, provider};
use academy_extern_contracts::vat::VatApiService;
use academy_extern_impl::vat::{VatApiServiceConfig, VatApiServiceImpl};

#[tokio::test]
async fn valid() {
    let sut = make_sut();
    let result = sut.is_vat_id_valid("DE0123456789").await.unwrap();
    assert!(result);
}

#[tokio::test]
async fn invalid_regex_mismatch() {
    let sut = make_sut();
    let result = sut.is_vat_id_valid("012345").await.unwrap();
    assert!(!result);
}

#[tokio::test]
async fn invalid_api_rejection() {
    let sut = make_sut();
    let result = sut.is_vat_id_valid("DE54321").await.unwrap();
    assert!(!result);
}

fn make_sut() -> VatApiServiceImpl {
    let config = academy_config::load().unwrap();

    provider! {
        Provider { vat_api_service_config: VatApiServiceConfig, }
    }

    let mut provider = Provider {
        _cache: Default::default(),
        vat_api_service_config: VatApiServiceConfig::new(config.vat.validate_endpoint_override),
    };

    provider.provide()
}
