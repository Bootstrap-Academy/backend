# Configuration Reference

The backend is configured through TOML files listed in the `ACADEMY_CONFIG` environment variable (see [`ARCHITECTURE.md`](../ARCHITECTURE.md#configuration)).
[`config.toml`](../config.toml) in the repository root is always loaded first and holds the defaults; every file named in `ACADEMY_CONFIG` overrides it, files listed earlier taking priority over later ones.
Run `academy check-config --verbose` to validate the resulting configuration and print it.

This page lists every property the backend reads.
Properties marked **required** have no default and must be set by the deployment.
Durations are strings built from `d`, `h`, `m` and `s` parts, e.g. `"30d"`, `"10m"` or `"1h 30m"`.

## `[http]`
| Property | Default | Description |
| --- | --- | --- |
| `address` | **required** | Socket address the API server binds to, e.g. `"0.0.0.0:80"`. |
| `real_ip.header` | *unset* | Header to read the client ip from when running behind a reverse proxy, e.g. `"X-Real-Ip"`. Without it the socket address of the TCP client is used. |
| `real_ip.set_from` | **required** with `real_ip.header` | Address of the reverse proxy. The header is read only when the request comes from it; requests from anywhere else keep their socket address, so a client cannot choose its own ip. |
| `allowed_origins` | `[]` | List of regular expressions matching the origins that are allowed by CORS. |

## `[database]`
| Property | Default | Description |
| --- | --- | --- |
| `url` | **required** | Postgres connection string ([format](https://docs.rs/tokio-postgres/latest/tokio_postgres/config/struct.Config.html)). |
| `max_connections` | `10` | Maximum size of the connection pool. |
| `min_connections` | `0` | Number of idle connections the pool keeps open. |
| `acquire_timeout` | `"10s"` | Time to wait for a connection from the pool. |
| `idle_timeout` | `"10m"` | Time after which an idle connection is closed. Omit to disable. |
| `max_lifetime` | `"30m"` | Maximum lifetime of a connection. Omit to disable. |
| `run_migrations` | `true` | Run pending database migrations on startup. |

## `[cache]`
| Property | Default | Description |
| --- | --- | --- |
| `url` | **required** | Valkey/Redis connection string ([format](https://docs.rs/redis/latest/redis/#connection-parameters)). |
| `max_connections` | `10` | Maximum size of the connection pool. |
| `min_connections` | `0` | Number of idle connections the pool keeps open. |
| `acquire_timeout` | `"10s"` | Time to wait for a connection from the pool. |
| `idle_timeout` | `"10m"` | Time after which an idle connection is closed. Omit to disable. |
| `max_lifetime` | `"30m"` | Maximum lifetime of a connection. Omit to disable. |

## `[email]`
| Property | Default | Description |
| --- | --- | --- |
| `smtp_url` | **required** | SMTP connection string ([format](https://docs.rs/lettre/latest/lettre/transport/smtp/struct.AsyncSmtpTransport.html#method.from_url)). |
| `from` | **required** | Sender of all outgoing emails, e.g. `"Bootstrap Academy <noreply@example.com>"`. |

## `[jwt]`
| Property | Default | Description |
| --- | --- | --- |
| `secret` | **required** | Secret used to sign and verify JSON Web Tokens. |
| `download_token_ttl` | `"10m"` | Lifetime of the tokens issued for file downloads. |

## `[internal]`
| Property | Default | Description |
| --- | --- | --- |
| `jwt_ttl` | `"10s"` | Lifetime of the tokens the backend issues to authenticate itself against the microservices. |

### `[internal.secrets]`
One secret per audience of the internal service tokens, so that a token issued for one service cannot be used against another.
An audience that is not listed here falls back to `jwt.secret`, on signing and on verification alike, which keeps the shared secret in use until a dedicated one has been deployed to every sender and to the receiver.
The audiences are `auth` and `shop` (incoming, verified by the `_internal` endpoints of this backend) and `skills`, `challenges` and `events` (outgoing, expected by the microservices).

| Property | Default | Description |
| --- | --- | --- |
| `<audience>` | *unset* | Secret used to sign and verify the internal service tokens of that audience. |

## `[health]`
| Property | Default | Description |
| --- | --- | --- |
| `database_cache_ttl` | `"10s"` | How long a database health check result is reused. |
| `cache_cache_ttl` | `"10s"` | How long a cache health check result is reused. |
| `email_cache_ttl` | `"10s"` | How long an email health check result is reused. |

## `[user]`
| Property | Default | Description |
| --- | --- | --- |
| `terms_version` | `"2026-09"` | Version of the terms and conditions that is currently in force. `POST /auth/users` and `POST /auth/users/{user_id}/terms` accept only this version and record it; any other value is rejected with `422`. Has to match `TERMS_VERSION` in the frontend, which decides when the acceptance is asked for. |
| `name_change_rate_limit` | `"30d"` | Minimum time between two changes of a user name. |
| `export_rate_limit` | `"10m"` | Minimum time between two data exports (`GET /auth/users/{user_id}/export`) of the same user. Administrators are exempt. |
| `verification_code_ttl` | `"4h"` | Lifetime of an email verification code. |
| `verification_redirect_url` | `"https://bootstrap.academy/auth/verify-account"` | Target of the link in the verification email. |
| `password_reset_code_ttl` | `"4h"` | Lifetime of a password reset code. |
| `password_reset_redirect_url` | `"https://bootstrap.academy/auth/reset-password"` | Target of the link in the password reset email. |

## `[session]`
| Property | Default | Description |
| --- | --- | --- |
| `access_token_ttl` | `"5m"` | Lifetime of an access token. |
| `refresh_token_ttl` | `"30d"` | Lifetime of a refresh token. Sessions that have not been refreshed within this period are removed by `academy task prune-database`. |
| `refresh_token_length` | `64` | Length of the generated refresh tokens in bytes. |
| `login_fails_before_captcha` | `3` | Number of failed logins after which a captcha is requested. Only relevant while reCAPTCHA is enabled. |

## `[totp]`
| Property | Default | Description |
| --- | --- | --- |
| `secret_length` | `32` | Length of generated TOTP secrets in bytes. |

## `[contact]`
| Property | Default | Description |
| --- | --- | --- |
| `email` | **required** | Recipient of the contact form and of the internal notifications about contract declarations. |

## `[contract]`
Rate limits of the consumer declaration endpoints (`POST /contracts/cancellations` and `POST /contracts/withdrawals`), counted per client ip address and per email address over the same window.
Both counters are shared between the two endpoints, and a request that is refused is not counted.

The per-ip budget only stops a single machine from flooding the endpoint, so it is deliberately much larger than the per-address one: everybody behind the same NAT — a household, a school, a carrier's CGNAT — shares one ip address, and § 312k Abs. 2 BGB requires the cancellation form to be permanently available to each of them.

| Property | Default | Description |
| --- | --- | --- |
| `rate_limit_window` | `"1h"` | Lifetime of a counter. It starts again with every counted request, so the window slides rather than being reset at a fixed point. |
| `rate_limit_per_ip` | `60` | Maximum number of declarations per client ip address within the window. |
| `rate_limit_per_email` | `5` | Maximum number of declarations per email address within the window. |

## `[recaptcha]`
The section is always parsed, so `sitekey` and `secret` have to be set even when the check is switched off.

| Property | Default | Description |
| --- | --- | --- |
| `enable` | `true` | Set to `false` to drop the whole section: no reCAPTCHA response is verified and `GET /auth/recaptcha` reports no site key, so the frontend does not load the reCAPTCHA script. |
| `siteverify_endpoint_override` | *unset* | Alternative siteverify endpoint, used by the test setup. |
| `sitekey` | **required** | reCAPTCHA site key, published through `GET /auth/recaptcha`. |
| `secret` | **required** | reCAPTCHA secret key. |
| `min_score` | `0.5` | Minimum score a reCAPTCHA response has to reach. |

## `[vat]`
| Property | Default | Description |
| --- | --- | --- |
| `validate_endpoint_override` | *unset* | Alternative VIES endpoint for VAT id validation, used by the test setup. |

## `[paypal]`
| Property | Default | Description |
| --- | --- | --- |
| `base_url_override` | *unset* | Alternative PayPal API base url, used by the test setup. |
| `client_id` | **required** | PayPal client id, published through `GET /shop/coins/paypal`. |
| `client_secret` | **required** | PayPal client secret. |

## `[coin]`
| Property | Default | Description |
| --- | --- | --- |
| `purchase_min` | `500` | Smallest number of Morphcoins that can be bought in one order. |
| `purchase_max` | `1000000` | Largest number of Morphcoins that can be bought in one order. |

## `[heart]`
| Property | Default | Description |
| --- | --- | --- |
| `max` | `6` | Maximum number of hearts a user can hold. |
| `refill_price` | `50` | Price of a heart refill in Morphcoins. |
| `auto_refill_time` | `"00:00"` | Time of day in UTC at which hearts are refilled automatically. |

## `[premium]`
| Property | Default | Description |
| --- | --- | --- |
| `monthly_price` | `1000` | Price of one month of premium in Morphcoins. Automatic renewals always use this plan. |
| `yearly_price` | `10000` | Price of one year of premium in Morphcoins. |

## `[render]`
| Property | Default | Description |
| --- | --- | --- |
| `daemon_url` | **required** | Base url of the render daemon (`academy_render_daemon`), which renders HTML to PDF. |
| `timeout` | `"30s"` | Timeout of a single render request. A daemon that accepts the connection and then stops answering would otherwise keep the waiting request open indefinitely. |

## `[microservices]`
Base urls of the microservices the backend calls over the internal API, to propagate account deletions (see [`ARCHITECTURE.md`](../ARCHITECTURE.md#account-deletion)) and to collect the data export of a user (see [`ARCHITECTURE.md`](../ARCHITECTURE.md#data-export)).
A microservice without a url is skipped; an empty string counts as no url.
Skipped means silently skipped: account deletions are not propagated to it, and its share of a data export is simply absent — the export does not list it as unavailable and still reports itself as complete, because "no url" means "this service is not part of this deployment".
Which services are configured and which are not is logged when the backend starts.

| Property | Default | Description |
| --- | --- | --- |
| `skills_url` | *unset* | Base url of [skills-ms](https://github.com/Bootstrap-Academy/skills-ms). |
| `challenges_url` | *unset* | Base url of [challenges-ms](https://github.com/Bootstrap-Academy/challenges-ms). |
| `events_url` | *unset* | Base url of [events-ms](https://github.com/Bootstrap-Academy/events-ms). |
| `timeout` | `"10s"` | Timeout of a single internal request. |
| `export_timeout` | `"30s"` | Timeout of a single export request, which reads more data than a deletion. |
| `max_export_size` | `33554432` | Maximum size in bytes of the export response of a single microservice. A larger response is not buffered; the service is reported as unavailable in the export instead. |

## `[finance]`
| Property | Default | Description |
| --- | --- | --- |
| `vat_percent` | `19` | VAT rate in percent, published through `GET /shop/coins/config`. Has to be at least `0` and less than `100`; the backend refuses to start otherwise, because the net amounts of an invoice are derived from the gross amount that was paid. |
| `invoices_archive` | **required** | Directory the generated invoices are written to. |
| `credit_notes_archive` | **required** | Directory the generated credit notes are written to. |
| `final_statements_archive` | **required** | Directory the final statements issued at account deletion are written to. |
| `retention_years` | `8` | Number of years invoices, credit notes and final statements are kept, counted from the end of the calendar year in which they were issued. Enforced by `academy task prune-documents`. |

## `[sentry]`
Optional section for error reporting to GlitchTip/Sentry. It is not present in `config.toml`, so error reporting is off unless the deployment adds it.

| Property | Default | Description |
| --- | --- | --- |
| `enable` | `true` | Set to `false` to drop the whole section. |
| `dsn` | **required** | DSN of the GlitchTip/Sentry project. |

## `[oauth2]`
| Property | Default | Description |
| --- | --- | --- |
| `enable` | `true` | Set to `false` to disable OAuth2 entirely. OAuth2 is also disabled if no provider remains enabled. |
| `registration_token_ttl` | `"10m"` | Lifetime of the token issued after an OAuth2 login without a linked account. |
| `authorization_ttl` | `"10m"` | How long a started authorization flow (`POST /auth/oauth/authorize`) can be completed. It only has to cover the round trip through the provider's consent screen. |

### `[oauth2.providers.<id>]`
`config.toml` predefines `github`, `discord` and `google` with everything except the credentials; further providers can be added under any id.

Every provider is parsed before the disabled ones are dropped, so `client_id` and `client_secret` have to be set even for a provider with `enable = false` — an empty string is enough.

| Property | Default | Description |
| --- | --- | --- |
| `enable` | `true` | Set to `false` to remove the provider. |
| `name` | **required** | Display name of the provider. |
| `client_id` | **required** | OAuth2 client id. Required even when the provider is disabled. |
| `client_secret` | **required** | OAuth2 client secret. Required even when the provider is disabled. |
| `pkce` | `true` | Whether the provider supports PKCE (RFC 7636). Set to `false` only for a provider that rejects `code_challenge`. |
| `auth_url` | **required** | Authorization endpoint. |
| `token_url` | **required** | Token endpoint. |
| `userinfo_url` | **required** | Endpoint returning the profile of the authenticated user. |
| `userinfo_id_key` | **required** | Key of the user id in the userinfo response. |
| `userinfo_name_key` | **required** | Key of the user name in the userinfo response. |
| `scopes` | **required** | Scopes to request, e.g. `["identify"]`. |
