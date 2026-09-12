use super::*;
use serde_json::json;

const CASE: &str = "00000000-0000-4000-8000-000000000123";

fn message(body: Value) -> Value {
    json!({
        "id":"00000000-0000-4000-8000-000000000456", "case_id": CASE,
        "source":"challenges", "audience":"author", "body":body
    })
}

fn render(message: &Value) -> Email {
    compose(
        message,
        None,
        "Display name is not verified <fixture@example.invalid>"
            .parse()
            .unwrap(),
    )
}

#[test]
fn report_receipt_has_real_context_without_claiming_a_human_started_work() {
    let mut m = message(json!({"status":"received",
        "text":"Deine Meldung ist eingegangen. Ein Mensch prüft sie.",
        "redress":"Andere Rechtsbehelfe bleiben unberührt."}));
    m["audience"] = json!("notifier");
    m["email_context"] = json!({"target_kind":"subtask","target_title":"Schleifen mit Python"});
    m["recipient_name"] = json!("Ada");
    let email = render(&m);
    assert_eq!(
        email.subject,
        "Bootstrap Academy: Deine Meldung ist angekommen"
    );
    assert!(email.body.starts_with(
        "Hey Ada,\n\ndeine Meldung zu einer Aufgabe aus „Schleifen mit Python“ ist angekommen."
    ));
    assert!(email.body.contains(INBOX));
    assert!(!email.body.contains("Ein Mensch prüft"));
    assert!(!email.body.contains("Rechtsbehelfe"));
    assert!(!email.subject.contains(CASE));
    assert!(email.body.find(CASE).unwrap() > email.body.find(INBOX).unwrap());
    assert!(email.body.contains("Bereich Inhalte, Vorgangsnummer"));
}

#[test]
fn complaint_receipt_does_not_invent_a_result_or_reviewer() {
    let m = message(json!({"status":"complaint_received", "decision_id":CASE,
        "text":"Deine Beschwerde ist eingegangen und wartet auf menschliche Überprüfung."}));
    let email = render(&m);
    assert!(
        email
            .body
            .starts_with("Hallo,\n\ndeine Beschwerde ist angekommen.")
    );
    // This receipt can arrive after a later decision: do not assert current
    // absence of a decision from the older immutable receipt alone.
    assert!(!email.body.contains("liegt noch nicht vor"));
    assert!(!email.body.contains("Mensch"));
    assert!(!email.body.contains(CASE));
}

#[test]
fn provisional_content_notice_has_title_and_exact_reason_without_a_final_finding() {
    let reason = "Die Lösung nennt für 2 + 2 den Wert 5. Die Meldung allein ist kein festgestellter Regelverstoß.";
    let mut m = message(json!({"target_kind":"subtask", "outcome":"provisional",
        "rationale":reason, "ground":"Vorläufige Qualitätsprüfung nach AGB 14.3.",
        "human_review":false, "review_assessment":"This field alone is not proof of a human review.",
        "automation":"Die strukturierte Meldung löste die Ausblendung automatisch aus."}));
    m["email_context"] = json!({"target_kind":"subtask","target_title":"Addition in Python"});
    let email = render(&m);
    assert!(
        email
            .body
            .contains("deine Aufgabe aus „Addition in Python“ wurde vorläufig ausgeblendet.")
    );
    assert!(email.body.contains(reason));
    assert!(
        email
            .body
            .contains("Damit ist noch kein Regelverstoß festgestellt.")
    );
    assert!(
        email
            .body
            .contains("Grundlage: Vorläufige Qualitätsprüfung nach AGB 14.3.")
    );
    assert!(!email.body.contains("This field"));
    assert!(!email.body.contains("Automatisierung:"));
    assert!(!email.body.contains("legacy_observed"));
    assert!(!email.body.contains("review_assessment"));
    assert!(!email.body.contains("wurde automatisch"));
    m["email_context"]["decision_automatic"] = json!(true);
    assert!(render(&m).body.contains(
        "deine Aufgabe aus „Addition in Python“ wurde automatisch vorläufig ausgeblendet."
    ));
}

#[test]
fn account_restriction_explains_effect_reason_end_and_available_access() {
    let mut m = message(json!({"target_kind":"account", "outcome":"restrict",
        "rationale":"Über dein Konto wurden wiederholt beleidigende Beiträge veröffentlicht.",
        "ends_at":"2026-10-04T14:30:00+02:00"}));
    m["source"] = json!("backend");
    let email = render(&m);
    assert!(email.body.contains("dein Kontozugang wurde eingeschränkt."));
    assert!(email.body.contains("wiederholt beleidigende Beiträge"));
    assert!(email.body.contains("04.10.2026 um 12:30 Uhr (UTC)"));
    assert!(email.body.contains("kostenlos eine Überprüfung anfordern"));
    assert!(email.body.contains(ACCESS));
    assert!(email.body.contains("Bereich Konto, Vorgangsnummer"));
}

#[test]
fn functional_restrictions_are_not_presented_as_an_account_ban() {
    for (kind, expected) in [("create", "Erstellen"), ("report", "Melden")] {
        let email = render(&message(json!({"target_kind":kind,"outcome":"restrict"})));
        assert!(email.body.contains(&format!(
            "das {expected} von Aufgaben mit deinem Konto wurde eingeschränkt."
        )));
        assert!(!email.body.contains("dein Kontozugang wurde eingeschränkt"));
    }
}

#[test]
fn restoration_and_historical_review_do_not_claim_full_reactivation() {
    let mut m = message(json!({"target_kind":"subtask","outcome":"restore",
        "rationale":"Die festgelegte Dauer ist abgelaufen.", "effective":{"enabled":false,"retired":true}}));
    let email = render(&m);
    assert!(
        email
            .body
            .contains("die Einschränkung für deine Aufgabe aus diesem Vorgang ist beendet.")
    );
    assert!(!email.body.contains("wieder verfügbar"));
    assert!(!email.body.contains("Mensch"));
    m["body"] = json!({"target_kind":"subtask","outcome":"uphold","historical_only":true,
        "ends_at":"2025-01-01T00:00:00Z", "human_review":true,
        "review_assessment":"Die damalige Entscheidung bleibt bestehen; die Aufgabe wurde später zurückgezogen."});
    let email = render(&m);
    assert!(
        email
            .body
            .contains("Dadurch entsteht keine neue Einschränkung.")
    );
    assert!(
        email
            .body
            .contains("Ergebnis der Überprüfung: Die damalige Entscheidung")
    );
    assert!(!email.body.contains("Diese Einschränkung gilt bis"));
}

#[test]
fn notifier_decision_never_accuses_the_notifier_of_the_reported_action() {
    let mut m = message(
        json!({"outcome":"remove", "text":"Die gemeldete Aufgabe wurde wegen ihrer falschen Musterlösung entfernt."}),
    );
    m["audience"] = json!("notifier");
    m["email_context"] = json!({"target_kind":"subtask","target_title":"Division"});
    let email = render(&m);
    assert!(
        email
            .body
            .contains("die von dir gemeldete Aufgabe aus „Division“ wurde entfernt.")
    );
    assert!(!email.body.contains("deine Aufgabe"));
    assert!(!email.body.contains("dein Kontozugang wurde"));
    for kind in ["account", "create", "report"] {
        m["email_context"] = json!({"target_kind":kind});
        m["body"]["outcome"] = json!("restrict");
        let email = render(&m);
        assert!(!email.body.contains("dein Kontozugang wurde"));
        assert!(!email.body.contains("mit deinem Konto wurde"));
    }
}

#[test]
fn authority_notice_preserves_specific_appeal_route() {
    let redress = "Für einen Rechtsbehelf gegen diese Anordnung beachte bitte die Frist im zugestellten Bescheid; zuständig ist die dort benannte Stelle.";
    let email = render(&message(
        json!({"target_kind":"account", "outcome":"authority_start", "redress":redress}),
    ));
    assert!(email.body.contains("behördlich angeordnete Einschränkung"));
    assert!(email.body.contains(redress));
}

#[test]
fn requested_recovery_keeps_exact_capability_and_scope_without_decision_boilerplate() {
    let link =
        "https://bootstrap.academy/moderation/access#capability=fixture-only-never-a-real-token";
    let mut m = message(json!({"recovery_link":link, "expires_at":"2026-09-12T09:00:00Z"}));
    m["audience"] = json!("recovery");
    let email = render(&m);
    assert_eq!(email.subject, "Bootstrap Academy: Dein Zugangslink");
    assert!(email.body.contains(link));
    assert!(email.body.contains("12.09.2026 um 09:00 Uhr (UTC)"));
    assert!(
        email
            .body
            .contains("Damit kannst du diesen Vorgang lesen und eine Überprüfung anfordern.")
    );
    assert!(
        email
            .body
            .contains("Wenn du ihn nicht angefordert hast, musst du nichts tun.")
    );
    assert!(!email.body.contains("Rechtsbehelfe"));
    assert!(!email.body.contains("Mensch"));
    assert!(!email.body.contains(CASE));
}

#[test]
fn missing_context_and_untrusted_contact_name_do_not_create_a_title_or_identity() {
    let mut m = message(json!({"status":"received","recipient_name":"untrusted body name"}));
    let email = render(&m);
    assert!(
        email
            .body
            .starts_with("Hallo,\n\ndeine Meldung ist angekommen.")
    );
    assert!(!email.body.contains("Display name"));
    assert!(!email.body.contains("untrusted body name"));
    m["recipient_name"] = json!("Name\nInjected line");
    assert!(render(&m).body.starts_with("Hallo,"));
}

#[test]
fn technical_fields_do_not_become_customer_copy_or_change_transport_identity() {
    let mut m = message(
        json!({"target_kind":"subtask", "outcome":"warn", "decision_id":CASE,
        "rule_version":"internal-rule-v5", "scope":"Diese Aufgabe auf Bootstrap Academy",
        "private_evidence":{"email":"private@example.invalid"}}),
    );
    m["email_context"] = json!({"target_title":"A\nB"});
    let email = render(&m);
    for internal in [
        CASE,
        "private@example.invalid",
        "internal-rule-v5",
        "Umfang:",
        "Regelfassung:",
    ] {
        assert!(!email.body.contains(internal));
    }
    assert!(email.body.contains("„A B“"));
    assert_eq!(
        email.message_id.as_deref(),
        Some("moderation-challenges-00000000-0000-4000-8000-000000000456@bootstrap.academy")
    );
    assert_eq!(email.content_type, ContentType::Text);
    assert!(email.attachments.is_empty());
}
