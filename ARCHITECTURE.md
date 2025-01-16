# Bootstrap Academy Backend Architecture
This document aims to provide a high-level overview over the architecture of the backend.

## Components
The backend currently consists of the following components:

- The Rust backend monolith found in this repository
- The old Python/Rust microservices (soon to be integrated into the new backend): [skills-ms](https://github.com/Bootstrap-Academy/skills-ms), [jobs-ms](https://github.com/Bootstrap-Academy/jobs-ms), [events-ms](https://github.com/Bootstrap-Academy/events-ms), [challenges-ms](https://github.com/Bootstrap-Academy/challenges-ms)
- A [PostgreSQL](https://www.postgresql.org/) database for persistence
- A [Valkey](https://valkey.io/)/[Redis](https://redis.io/) server for caching
- External services/APIs:
    - An SMTP server for sending emails
    - [Google reCAPTCHA](https://developers.google.com/recaptcha/intro)
    - Various OAuth2 providers like GitHub, Discord, Google, ...
    - [Vies on-the-Web](https://ec.europa.eu/taxation_customs/vies/#/technical-information) for VAT validation
    - [GlitchTip](https://glitchtip.com/)/[Sentry](https://sentry.io/) for error tracking and monitoring

## Important Crates
- Async runtime: [`tokio`](https://docs.rs/tokio)
- Error handling: [`anyhow`](https://docs.rs/anyhow), [`thiserror`](https://docs.rs/thiserror)
- CLI: [`clap`](https://docs.rs/clap)
- Tracing: [`tracing`](https://docs.rs/tracing)
- Date and time: [`chrono`](https://docs.rs/chrono)
- Newtypes: [`nutype`](https://docs.rs/nutype)
- Serialization and deserialization: [`serde`](https://docs.rs/serde)
- Mocking for unit tests: [`mockall`](https://docs.rs/mockall)
- HTTP client: [`reqwest`](https://docs.rs/reqwest)
- HTTP server: [`axum`](https://docs.rs/axum)
- REST API documentation: [`aide`](https://docs.rs/aide)
- Postgres client: [`bb8-postgres`](https://docs.rs/bb8-postgres) / [`tokio-postgres`](https://docs.rs/tokio-postgres)
- Valkey/Redis client: [`bb8-redis`](https://docs.rs/bb8-redis) / [`redis`](https://docs.rs/redis)
- Email: [`lettre`](https://docs.rs/lettre)

## Features

### REST API
The main purpose of the backend is to serve the REST API used by the frontend and potentially other systems integrating with Bootstrap Academy.
To start the API server, the `academy serve` command is used, which causes the backend to bind a TCP listener to the configured address and respond to HTTP requests.
In a production environment it is expected to deploy the REST API server behind a reverse proxy like nginx which handles TLS termination.

#### Documentation
An [OpenAPI specification](https://swagger.io/specification/) is automatically generated and served at `/openapi.json`.
In addition, both [Swagger UI](https://swagger.io/tools/swagger-ui/) and [Redoc](https://redocly.com/redoc) are available on `/docs` and `/redoc` respectively.

#### Authentication
Clients are mostly authenticated using JWTs:

- Normal users logging in with their account credentials receive an access token (JWT) and a refresh token (random opaque secret) and use the access token to authenticate all subsequent requests. When the access token expires (or is invalidated) the client uses the refresh token to request a new access/refresh token pair which replaces the current one.
- Services (esp. the old Python/Rust microservices) authenticate each request by issuing a very short-lived JWT which includes the target audience (the recipient of the request).

#### Tracing
Each incoming request is assigned a unique request id (Base64 encoded UUIDv7).
This id is automatically attached to any logs associated with the corresponding request and is also returned to the client in the `X-Request-Id` response header.

### Scheduled Tasks
There are some tasks that need to run on a regular basis (e.g. removing expired sessions from the database).
Instead of implementing a scheduler directly in the backend daemon, we rely on external schedulers (e.g. systemd timers or cron jobs) that invoke subcommands of `academy task` to start the corresponding tasks (e.g. `academy task prune-database`).

### CLI
The `academy` executable also provides some other useful commands e.g. for administration, debugging and testing purposes.

## Configuration
The backend is configured using one or more TOML files specified in the `ACADEMY_CONFIG` environment variable.
This environment variable contains a `:`-separated list of paths to config files with decreasing priority (i.e. properties set by the first file override those of all other files).
The default configuration found in `config.toml` is always loaded implicitly with minimum priority.
Usually defaults should be set for all properties except for those that depend on the deployment environment (e.g. database url) or that contain secrets or other sensitive information.
Inside the development environment the `ACADEMY_CONFIG` environment variable is automatically set to point to the `config.dev.toml` config file.

## Hexagonal Architecture
The Bootstrap Academy backend follows the [Hexagonal Architecture](https://en.wikipedia.org/wiki/Hexagonal_architecture_(software)) approach.
Each component is contained within its own crate, for example:
- `academy_core`: The application core. Each sub-crate contains one feature (collection of related use cases).
- `academy_models`: Entities and newtypes used throughout the application.
- `academy_shared`: Various helper services that are not directly related to any feature (e.g. id service, time service, ...).
- `academy_persistence`: Database adapters and repositories.
- `academy_extern`: Adapters for external APIs.
- `academy_api`: The API server implementation.
- `academy`: The `academy` CLI.

### Services
A service refers to both a *trait* that declares methods for some functionality and an *implementation* of this trait.
Service traits always live in `*_contracts` crates, and implementations live in `*_impl` crates (if there is only one real implementation).
For example in `academy_shared_contracts` there is an `IdService` trait which is implemented for `IdServiceImpl` in `academy_shared_impl`.
Whenever this service is needed, only a dependency on the `IdService` *trait* is required ([Dependency Inversion](https://en.wikipedia.org/wiki/Dependency_inversion_principle)).
This approach allows easily swapping out the implementation for some service which is especially useful for unit testing where dependencies are usually replaced by mocks.

#### Traits
Service traits usually have `Send + Sync + 'static` as a supertrait bound so they can be used in a multi-threaded async executor like tokio.
Additionally, any `async` method within these traits needs a `Send` bound on the returned `Future`:

```rust
trait FooService {
    fn foo(&self, x: i32) -> impl Future<Output = i32> + Send;
    // instead of
    // async fn foo(&self, x: i32) -> i32;
}
```

If a service needs to be mocked within a unit test, the `automock` macro from [`mockall`](https://docs.rs/mockall) can be used on the service trait:
```rust
#[cfg_attr(feature = "mock", mockall::automock)]
trait FooService {
    fn foo(&self, x: i32) -> impl Future<Output = i32> + Send;
}

// Helper methods to make mock construction in unit tests more readable
#[cfg(feature = "mock")]
impl MockFooService {
    pub fn with_foo(mut self, x: i32, result: i32) -> Self {
        self.expect_foo()
            .once()
            .with(mockall::predicate::eq(x))
            .return_once(move || result;
        self
    }
}
```

#### Implementations
Service implementors are usually structs which contain all services they depend on plus optionally some configuration and state.
Because service implementors are only allowed to depend on service traits (and not their implementations), a type parameter has to be introduced for each dependency on a service:

```rust
// the service implementor contains its dependencies (in this case only the `IdService`) as generics
struct FooServiceImpl<Id> {
    id: Id,
}

impl<Id> FooService for FooServiceImpl<Id>
where
    // in the implementation for `FooService` the type parameters are constrained to their corresponding service traits
    Id: IdService,
{
    async fn foo(&self) {
        // use the id service dependency
        let id = self.id.generate();

        // ...
    }
}
```

For each service its default implementor is set as a type alias in `academy/src/environment/types.rs`:

```rust
pub type Id = IdServiceImpl;  // no dependencies
pub type Foo = FooServiceImpl<Id>;  // `Id` here is a concrete type instead of a type parameter
```

### Dependency Injection
The `academy_di` crate implements the basics of [Dependency Injection](https://en.wikipedia.org/wiki/Dependency_injection).
Notably, it provides the `Build` and `Provider` traits and some macros to implement these traits.

The `Build` trait looks like this:

```rust
pub trait Build<P: Provider>: Clone + 'static {
    fn build(provider: &mut P) -> Self;
}
```

Given a provider `P`, the `build` method is expected to construct an instance of the type for which this trait is implemented.
For example, the `Build` implementations of `IdServiceImpl` and `FooServiceImpl` from above could look like this:

```rust
impl<P> Build<P> for IdServiceImpl
where
    P: Provider
{
    fn build(_provider: &mut P) -> Self {
        // no dependencies, so we simply return the unit struct
        IdServiceImpl
    }
}

impl<P, Id> Build<P> for FooServiceImpl<Id>
where
    P: Provider,
    Id: Build<P>,
{
    fn build(provider: &mut P) -> Self {
        // first build all the dependencies
        let id = <Id as Build<P>>::build(provider);

        // then construct Self
        FooServiceImpl { id }
    }
}
```

In practice, most of these implementations don't have to be written by hand but can be produced automatically by the `Build` derive macro:

```rust
#[derive(Debug, Clone, Build)]
struct IdServiceImpl;

#[derive(Debug, Clone, Build)]
struct FooServiceImpl<Id> {
    id: Id,
}
```

The real implementations generated by the derive macro also implement caching to avoid building the same service twice.
In most cases this should make no difference, but some services store some local state which should be created only once.
The cache is provided as a `TypeMap` by the `Provider` trait:

```rust
pub trait Provider: Sized {
    fn cache(&mut self) -> &mut TypeMap;
}
```

This trait is usually implemented by the `provider!` macro which defines a new struct, implements `Provider` for it and implements `Build` for all fields of it:

```rust
provider! {
    Provider {
        num: i32,
    }
}

// create the provider
let mut provider = Provider {
    _cache: Default::default(),
    num: 42,
};

// provide the i32
let num: i32 = provider.provide();

// provide Foo (type alias for FooServiceImpl<IdServiceImpl>)
let foo: Foo = provider.provide();
```

The main providers are defined in `academy/src/environment/mod.rs`.
