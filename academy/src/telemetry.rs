//! Error reports have their own privacy boundary, independent of stderr's level.

use std::sync::Arc;

use sentry::{
    Breadcrumb, ClientOptions, Level,
    protocol::{Context, Event},
};
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

/// Keep free-form structured input out of error reports. The message and
/// stacktrace still describe the failure; request IDs allow local correlation.
fn safe_field(key: &str) -> bool {
    matches!(key, "request_id" | "method" | "status" | "status_code")
}

fn smtp_logger(logger: &str) -> bool {
    logger == "lettre" || logger.starts_with("lettre::")
}

pub fn filter_breadcrumb(mut breadcrumb: Breadcrumb) -> Option<Breadcrumb> {
    if breadcrumb.level < Level::Info || breadcrumb.category.as_deref().is_some_and(smtp_logger) {
        return None;
    }
    breadcrumb
        .data
        .retain(|key, value| safe_field(key) && !value.is_array() && !value.is_object());
    Some(breadcrumb)
}

pub fn filter_event(mut event: Event<'static>) -> Option<Event<'static>> {
    if event.level < Level::Info || event.logger.as_deref().is_some_and(smtp_logger) {
        return None;
    }
    event
        .extra
        .retain(|key, value| safe_field(key) && !value.is_array() && !value.is_object());
    event.tags.retain(|key, _| safe_field(key));
    for context in event.contexts.values_mut() {
        if let Context::Other(fields) = context {
            fields.retain(|key, value| safe_field(key) && !value.is_array() && !value.is_object());
        }
    }
    event.breadcrumbs.values = event
        .breadcrumbs
        .values
        .into_iter()
        .filter_map(filter_breadcrumb)
        .collect();
    if let Some(request) = &mut event.request {
        request.data = None;
        request.cookies = None;
        request.query_string = None;
        request.headers.clear();
        request.env.clear();
        if let Some(url) = &mut request.url {
            url.set_query(None);
            url.set_fragment(None);
            let _ = url.set_username("");
            let _ = url.set_password(None);
        }
    }
    if let Some(user) = &mut event.user {
        user.email = None;
        user.username = None;
        user.other.clear();
    }
    Some(event)
}

pub fn options() -> ClientOptions {
    ClientOptions {
        attach_stacktrace: true,
        before_send: Some(Arc::new(filter_event)),
        before_breadcrumb: Some(Arc::new(filter_breadcrumb)),
        ..Default::default()
    }
}

pub fn init_tracing() {
    let fmt_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);

    #[cfg(tracing_pretty)]
    let fmt_layer = fmt_layer.pretty();

    tracing_subscriber::registry()
        .with(fmt_layer.with_filter(EnvFilter::from_default_env()))
        .with(sentry::integrations::tracing::layer().event_filter(|meta| {
            use sentry::integrations::tracing::EventFilter;
            if smtp_logger(meta.target()) {
                return EventFilter::Ignore;
            }
            match *meta.level() {
                tracing::Level::ERROR | tracing::Level::WARN => EventFilter::Event,
                tracing::Level::INFO => EventFilter::Breadcrumb,
                tracing::Level::DEBUG | tracing::Level::TRACE => EventFilter::Ignore,
            }
        }))
        .init();
}

#[cfg(test)]
mod tests {
    use sentry::protocol::{Request, User};
    use serde_json::json;

    use super::*;

    #[test]
    fn reports_drop_debug_smtp_and_arbitrary_structured_input() {
        assert!(
            filter_event(Event {
                level: Level::Debug,
                ..Default::default()
            })
            .is_none()
        );
        assert!(
            filter_breadcrumb(Breadcrumb {
                level: Level::Debug,
                ..Default::default()
            })
            .is_none()
        );
        assert!(
            filter_event(Event {
                logger: Some("lettre::transport::smtp".into()),
                ..Default::default()
            })
            .is_none()
        );
        let mut event = Event {
            message: Some("mail delivery failed".into()),
            extra: json!({"password":"secret", "mail":{"content":"code"}, "request_id":"request-42", "method": {"nested": "secret"}}).as_object().unwrap().clone().into_iter().collect(),
            request: Some(Request { data: Some("secret".into()), cookies: Some("secret".into()), query_string: Some("secret".into()), ..Default::default() }),
            user: Some(User { id: Some("user-42".into()), email: Some("private@example.com".into()), ..Default::default() }),
            ..Default::default()
        };
        event.contexts.insert(
            "Rust Tracing Fields".into(),
            Context::Other(
                json!({"token":"secret", "request_id":"request-42"})
                    .as_object()
                    .unwrap()
                    .clone()
                    .into_iter()
                    .collect(),
            ),
        );
        event.breadcrumbs.values.push(Breadcrumb {
            message: Some("request started".into()),
            data: json!({"confirmation_code":"secret", "request_id":"request-42"})
                .as_object()
                .unwrap()
                .clone()
                .into_iter()
                .collect(),
            ..Default::default()
        });
        let event = filter_event(event).unwrap();
        assert_eq!(event.message.as_deref(), Some("mail delivery failed"));
        assert_eq!(event.extra.len(), 1);
        assert_eq!(event.extra["request_id"], "request-42");
        assert_eq!(event.breadcrumbs.values[0].data.len(), 1);
        if let Context::Other(fields) = &event.contexts["Rust Tracing Fields"] {
            assert_eq!(fields.len(), 1);
            assert_eq!(fields["request_id"], "request-42");
        } else {
            panic!("tracing context changed type");
        }
        let request = event.request.unwrap();
        assert!(
            request.data.is_none() && request.cookies.is_none() && request.query_string.is_none()
        );
        let user = event.user.unwrap();
        assert_eq!(user.id.as_deref(), Some("user-42"));
        assert!(user.email.is_none());
    }
}
