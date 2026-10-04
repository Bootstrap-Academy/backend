//! Compare rendered credentials with real TRACE output in both build profiles.
use std::{
    io,
    sync::{Arc, Mutex},
};

use academy_di::Provide;
use academy_models::RecaptchaResponse;
use academy_templates_contracts::{ResetPasswordTemplate, TemplateService, VerifyEmailTemplate};
use academy_templates_impl::TemplateServiceImpl;

academy_di::provider! { Provider {} }

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn render_captured(reset: bool) {
    let mut provider = Provider {
        _cache: Default::default(),
    };
    let service: TemplateServiceImpl = provider.provide();
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let writer = Capture(Arc::clone(&bytes));
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::TRACE)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NEW)
        .with_writer(move || writer.clone())
        .finish();
    let code = "ABCD-EFGH-IJKL-MNOP";
    let url = format!("http://127.0.0.1:9/?code={code}");
    let (rendered, debug) = tracing::subscriber::with_default(subscriber, || {
        if reset {
            let template = ResetPasswordTemplate {
                code: code.into(),
                url,
            };
            (service.render(&template).unwrap(), format!("{template:?}"))
        } else {
            let template = VerifyEmailTemplate {
                code: code.into(),
                url,
            };
            (service.render(&template).unwrap(), format!("{template:?}"))
        }
    });
    assert!(
        rendered.contains(code),
        "rendered message lost its credential"
    );
    let logged = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
    let name = if reset {
        "ResetPasswordTemplate"
    } else {
        "VerifyEmailTemplate"
    };
    assert!(
        logged.contains(name) && logged.contains("render"),
        "safe span control missing"
    );
    assert!(
        !logged.contains(code),
        "template credential recorded in TRACE"
    );
    assert!(!debug.contains(code), "template Debug exposed a credential");
}

#[test]
fn password_reset_template_keeps_code_in_message_and_out_of_logs() {
    render_captured(true);
}

#[test]
fn email_confirmation_template_keeps_code_in_message_and_out_of_logs() {
    render_captured(false);
}

#[test]
fn captcha_response_and_server_config_debug_always_redact() {
    let response: RecaptchaResponse = "owned-synthetic-captcha-response".try_into().unwrap();
    let config = academy_shared_impl::captcha::RecaptchaCaptchaServiceConfig {
        sitekey: "public-sitekey".into(),
        secret: "owned-synthetic-captcha-server-key".into(),
        min_score: 0.5,
    };
    let debug = format!("{response:?} {config:?}");
    assert!(
        !debug.contains(&**response),
        "reCAPTCHA response exposed through Debug"
    );
    assert!(
        !debug.contains(&*config.secret),
        "reCAPTCHA server key exposed through Debug"
    );
    assert!(debug.contains("public-sitekey") && debug.contains("<redacted>"));
}
