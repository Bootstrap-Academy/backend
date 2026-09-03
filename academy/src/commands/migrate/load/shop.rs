use academy_models::{
    coin::Transaction,
    heart::Hearts,
    paypal::PaypalCoinOrder,
    premium::{Premium, PremiumPlan},
};
use academy_persistence_contracts::{
    Database, Transaction as _, coin::CoinRepository, heart::HeartRepository,
    paypal::PaypalRepository, premium::PremiumRepository, user::UserRepository,
};
use academy_persistence_postgres::{
    PostgresDatabase, coin::PostgresCoinRepository, heart::PostgresHeartRepository,
    paypal::PostgresPaypalRepository, premium::PostgresPremiumRepository,
    user::PostgresUserRepository,
};
use chrono::NaiveDateTime;
use indicatif::ProgressIterator;
use tracing::info;
use uuid::Uuid;

use super::DbConnection;

pub async fn load(db: PostgresDatabase, shop: DbConnection) -> anyhow::Result<()> {
    let mut txn = db.begin_transaction().await?;

    let user_repo = PostgresUserRepository;
    let coin_repo = PostgresCoinRepository;
    let paypal_repo = PostgresPaypalRepository;
    let heart_repo = PostgresHeartRepository;
    let premium_repo = PostgresPremiumRepository;

    info!("loading coins");
    for row in shop
        .query("select * from shop_coins", &[])
        .await?
        .into_iter()
        .progress()
    {
        let user_id: String = row.get("user_id");
        let coins: i64 = row.get("coins");
        let withheld_coins: i64 = row.get("withheld_coins");

        let Ok(user_id) = user_id.parse::<Uuid>().map(Into::into) else {
            continue;
        };

        if !user_repo.exists(&mut txn, user_id).await? {
            continue;
        }

        if coins != 0 {
            coin_repo.add_coins(&mut txn, user_id, coins, false).await?;
        }
        if withheld_coins != 0 {
            coin_repo
                .add_coins(&mut txn, user_id, withheld_coins, true)
                .await?;
        }
    }

    info!("loading transactions");
    for row in shop
        .query("select * from shop_transactions", &[])
        .await?
        .into_iter()
        .progress()
    {
        let id: String = row.get("id");
        let user_id: String = row.get("user_id");
        let created_at: NaiveDateTime = row.get("created_at");
        let coins: i64 = row.get("coins");
        let description: String = row.get("description");
        let credit_note: bool = row.get("credit_note");

        let user_id = user_id.parse::<Uuid>()?.into();

        if !user_repo.exists(&mut txn, user_id).await? {
            continue;
        }

        let transaction = Transaction {
            id: id.parse::<Uuid>()?.into(),
            user_id,
            coins,
            description: (!description.trim().is_empty())
                .then(|| description.try_into())
                .transpose()?,
            created_at: created_at.and_utc(),
            include_in_credit_note: credit_note,
        };
        coin_repo.create_transaction(&mut txn, &transaction).await?;
    }

    info!("loading user numbers");
    let mut max = 0;
    for row in shop
        .query("select * from shop_credit_note_users", &[])
        .await?
        .into_iter()
        .progress()
    {
        let user_id: String = row.get("user_id");
        let public_id: i64 = row.get("public_id");

        let user_id = user_id.parse::<Uuid>()?.into();

        if !user_repo.exists(&mut txn, user_id).await? {
            continue;
        }

        max = public_id.max(max);
        txn.txn()
            .execute(
                "insert into user_numbers (user_id, number) values ($1, $2)",
                &[&*user_id, &public_id],
            )
            .await?;
    }
    if max > 0 {
        txn.txn()
            .execute("select setval('user_number', $1)", &[&max])
            .await?;
    }

    info!("loading paypal coin orders");
    let max_invoice_no: i64 = shop
        .query_one("select max(invoice_no) from shop_paypal_orders", &[])
        .await?
        .get(0);
    txn.txn()
        .execute("select setval('invoice_number', $1)", &[&max_invoice_no])
        .await?;
    for row in shop
        .query("select * from shop_paypal_orders", &[])
        .await?
        .into_iter()
        .progress()
    {
        let id: String = row.get("id");
        let user_id: String = row.get("user_id");
        let created_at: NaiveDateTime = row.get("created_at");
        let coins: i64 = row.get("coins");
        let pending: bool = row.get("pending");
        let invoice_no: Option<i64> = row.get("invoice_no");

        let Ok(user_id) = user_id.parse::<Uuid>().map(Into::into) else {
            continue;
        };

        if !user_repo.exists(&mut txn, user_id).await? {
            continue;
        }

        let invoice_no = match invoice_no {
            Some(x) => x,
            None => txn
                .txn()
                .query_one("select nextval('invoice_number')", &[])
                .await?
                .get(0),
        };

        let coin_order = PaypalCoinOrder {
            id: id.try_into()?,
            user_id,
            created_at: created_at.and_utc(),
            captured_at: (!pending).then(|| created_at.and_utc()),
            coins: coins as _,
            invoice_number: invoice_no as _,
            withdrawal_consent_at: None,
            withdrawal_text_version: None,
        };

        paypal_repo.create_coin_order(&mut txn, &coin_order).await?;
    }

    info!("loading hearts");
    for row in shop
        .query("select * from shop_hearts", &[])
        .await?
        .into_iter()
        .progress()
    {
        let user_id: String = row.get("user_id");
        let hearts: i32 = row.get("hearts");
        let last_auto_refill: NaiveDateTime = row.get("last_auto_refill");

        let user_id = user_id.parse::<Uuid>()?.into();

        if !user_repo.exists(&mut txn, user_id).await? {
            continue;
        }

        let hearts = Hearts {
            hearts: hearts as _,
            last_refill: last_auto_refill.and_utc(),
        };

        heart_repo.set(&mut txn, user_id, hearts).await?;
    }

    info!("loading premium");
    for row in shop
        .query("select * from shop_premium", &[])
        .await?
        .into_iter()
        .progress()
    {
        let id: String = row.get("id");
        let user_id: String = row.get("user_id");
        let start: NaiveDateTime = row.get("start");
        let end: NaiveDateTime = row.get("end");

        let id = id.parse::<Uuid>()?.into();
        let user_id = user_id.parse::<Uuid>()?.into();

        if !user_repo.exists(&mut txn, user_id).await? {
            continue;
        }

        let premium = Premium {
            id,
            user_id,
            since: start.and_utc(),
            until: end.and_utc(),
        };

        premium_repo.create(&mut txn, premium).await?;
    }

    info!("loading premium subscriptions");
    for row in shop
        .query("select user_id, plan::text from shop_premium_autopay", &[])
        .await?
        .into_iter()
        .progress()
    {
        let user_id: String = row.get("user_id");
        let plan: String = row.get("plan");

        let user_id = user_id.parse::<Uuid>()?.into();

        if !user_repo.exists(&mut txn, user_id).await? {
            continue;
        }

        let plan = match plan.as_str() {
            "MONTHLY" => PremiumPlan::Monthly,
            "YEARLY" => PremiumPlan::Yearly,
            x => anyhow::bail!("invalid premium plan: {x}"),
        };

        premium_repo
            .set_subscription(&mut txn, user_id, Some(plan))
            .await?;
    }

    txn.commit().await?;

    Ok(())
}
