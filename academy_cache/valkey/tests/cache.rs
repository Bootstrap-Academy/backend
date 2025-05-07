use std::time::Duration;

use academy_cache_contracts::CacheService;
use academy_cache_valkey::{ValkeyCache, ValkeyCacheConfig};
use academy_demo::SHA256HASH1;
use academy_models::{Sha256Hash, email_address::EmailAddress};
use serde::{Deserialize, Serialize};

#[tokio::test]
async fn get() {
    let cache = setup().await;

    cache
        .set("foo", &"hello world".to_owned(), None)
        .await
        .unwrap();
    cache.set("bar", &42i32, None).await.unwrap();

    let foo = cache.get::<String>("foo").await.unwrap();
    let bar = cache.get::<i32>("bar").await.unwrap();
    let baz = cache.get::<char>("baz").await.unwrap();

    assert_eq!(foo.unwrap(), "hello world");
    assert_eq!(bar.unwrap(), 42);
    assert_eq!(baz, None);
}

#[tokio::test]
async fn set_no_ttl() {
    let cache = setup().await;

    assert_eq!(cache.get::<Vec<i32>>("foo").await.unwrap(), None);

    cache.set("foo", &vec![1i32, 3, 3, 7], None).await.unwrap();
    assert_eq!(
        cache.get::<Vec<i32>>("foo").await.unwrap().unwrap(),
        [1, 3, 3, 7]
    );

    cache.set("foo", &vec![4i32, 2], None).await.unwrap();
    assert_eq!(cache.get::<Vec<i32>>("foo").await.unwrap().unwrap(), [4, 2]);

    cache.set("foo", &*SHA256HASH1, None).await.unwrap();
    assert_eq!(
        cache.get::<Sha256Hash>("foo").await.unwrap().unwrap(),
        *SHA256HASH1
    );
}

#[tokio::test]
async fn set_ttl() {
    let cache = setup().await;

    assert!(cache.get::<()>("x").await.unwrap().is_none());

    cache
        .set("x", &(), Some(Duration::from_millis(200)))
        .await
        .unwrap();
    assert!(cache.get::<()>("x").await.unwrap().is_some());

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(cache.get::<()>("x").await.unwrap().is_some());

    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(cache.get::<()>("x").await.unwrap().is_none());
}

#[tokio::test]
async fn remove() {
    let cache = setup().await;

    assert!(cache.get::<()>("x").await.unwrap().is_none());

    cache
        .set("x", &(), Some(Duration::from_millis(200)))
        .await
        .unwrap();
    assert!(cache.get::<()>("x").await.unwrap().is_some());

    cache.remove("x").await.unwrap();
    assert!(cache.get::<()>("x").await.unwrap().is_none());

    cache.remove("x").await.unwrap();
    assert!(cache.get::<()>("x").await.unwrap().is_none());
}

#[tokio::test]
async fn types() {
    let cache = setup().await;

    macro_rules! tests {
        ($( $ty:ty: $val:expr),* $(,)? ) => {$({
            let key = stringify!($ty);
            let val = <$ty>::try_from($val).unwrap();
            cache.set(key, &val, None).await.unwrap();
            assert_eq!(cache.get::<$ty>(key).await.unwrap().unwrap(), val);
        })*};
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Struct {
        foo: i32,
        bar: String,
        baz: Enum,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    enum Enum {
        A,
        B,
        C,
    }

    tests! {
        (): (),
        i8: 1, u8: 2,
        i16: 3, u16: 4,
        i32: 5, u32: 6,
        i64: 7, u64: 8,
        i128: 9, u128: 10,
        isize: 9, usize: 10,
        bool: true, bool: false,
        char: '@',
        Vec<i32>: [2, 3, 5, 7, 11],
        Option<i32>: None, Option<i32>: Some(7),
        String: "Lorem ipsum dolor sit amet",
        Struct: Struct { foo: 17, bar: "hi there".into(), baz: Enum::B },
        EmailAddress: "foo@example.com",
        serde_json::Value: serde_json::Value::Number(42.into()),
    };
}

async fn setup() -> ValkeyCache {
    let config = academy_config::load().unwrap();

    let cache = ValkeyCache::connect(&ValkeyCacheConfig {
        url: config.cache.url,
        max_connections: config.cache.max_connections,
        min_connections: config.cache.min_connections,
        acquire_timeout: config.cache.acquire_timeout.into(),
        idle_timeout: config.cache.idle_timeout.map(Into::into),
        max_lifetime: config.cache.max_lifetime.map(Into::into),
    })
    .await
    .unwrap();
    cache.ping().await.unwrap();

    cache.clear().await.unwrap();

    cache
}
