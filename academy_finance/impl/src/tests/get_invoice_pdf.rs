use std::path::PathBuf;

use academy_demo::user::FOO;
use academy_finance_contracts::{
    coin::{CoinPrices, MockFinanceCoinService},
    FinanceService,
};
use academy_models::paypal::{PaypalCoinOrder, PaypalOrderId};
use academy_persistence_contracts::{paypal::MockPaypalRepository, user::MockUserRepository};
use academy_render_contracts::pdf::MockRenderPdfService;
use academy_shared_contracts::fs::MockFsService;
use academy_templates_contracts::{InvoiceItem, InvoiceTemplate, MockTemplateService};
use rust_decimal_macros::dec;

use crate::{tests::Sut, FinanceServiceImpl};

#[tokio::test]
async fn ok() {
    // Arrange
    let order = PaypalCoinOrder {
        id: PaypalOrderId::try_new("asdf1234").unwrap(),
        user_id: FOO.user.id,
        created_at: FOO.user.created_at,
        captured_at: None,
        coins: 1337,
        invoice_number: 42,
    };

    let pdf = vec![1, 2, 3, 4];

    let path = PathBuf::from("/invoices/R0000042.pdf");
    let fs = MockFsService::new()
        .with_read_file(path.clone(), None)
        .with_store_file(path, pdf.clone());

    let paypal_repo =
        MockPaypalRepository::new().with_get_coin_order_by_invoice_number(42, Some(order.clone()));

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let prices = CoinPrices {
        net_unit: 1.into(),
        net_total: 2.into(),
        vat_total: 3.into(),
        gross_total: 4.into(),
    };
    let finance_coin = MockFinanceCoinService::new().with_get_price(1337, prices);

    let template = MockTemplateService::new().with_render(
        InvoiceTemplate {
            title: "Rechnung",
            customer_details: FOO
                .invoice_info
                .clone()
                .into_details(Some(FOO.profile.display_name.clone().into_inner())),
            timestamp: order.created_at,
            invoice_number: "R0000042".into(),
            items: vec![InvoiceItem {
                description: "MorphCoins".into(),
                net_unit: prices.net_unit,
                count: order.coins,
                net_total: prices.net_total,
            }],
            vat_percent: dec!(19),
            net_total: prices.net_total,
            vat_total: prices.vat_total,
            gross_total: prices.gross_total,
            _static: Default::default(),
        },
        "invoice-template-html".into(),
    );

    let render_pdf =
        MockRenderPdfService::new().with_render("invoice-template-html".into(), pdf.clone());

    let sut = FinanceServiceImpl {
        fs,
        paypal_repo,
        user_repo,
        render_pdf,
        finance_coin,
        template,
        ..Sut::default()
    };

    // Act
    let result = sut.get_invoice_pdf(&mut (), 42).await.unwrap();

    // Assert
    assert_eq!(result, Some(pdf));
}

#[tokio::test]
async fn cached() {
    // Arrange
    let pdf = vec![1, 2, 3, 4];

    let fs =
        MockFsService::new().with_read_file("/invoices/R0000042.pdf".into(), Some(pdf.clone()));

    let sut = FinanceServiceImpl {
        fs,
        ..Sut::default()
    };

    // Act
    let result = sut.get_invoice_pdf(&mut (), 42).await.unwrap();

    // Assert
    assert_eq!(result, Some(pdf));
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let fs = MockFsService::new().with_read_file("/invoices/R0000042.pdf".into(), None);

    let paypal_repo = MockPaypalRepository::new().with_get_coin_order_by_invoice_number(42, None);

    let sut = FinanceServiceImpl {
        fs,
        paypal_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.get_invoice_pdf(&mut (), 42).await.unwrap();

    // Assert
    assert_eq!(result, None);
}
