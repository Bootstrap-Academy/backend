use academy_email_contracts::{ContentType, Email};
use academy_models::email_address::EmailAddressWithName;
use serde_json::Value;

const INBOX: &str = "https://bootstrap.academy/moderation";
const ACCESS: &str = "https://bootstrap.academy/moderation/access";
const AUTOMATIC_EXPIRY_REASON: &str = "Das gespeicherte Ende dieser Einschränkung ist erreicht. Diese Einschränkung wird beendet. Andere Einschränkungen und ein Rückzug durch den Autor bleiben maßgeblich.";
const AUTOMATIC_EXPIRY_GROUND: &str = "Ablauf der zuvor ausdrücklich festgelegten Dauer; keine neue Prüfung des ursprünglichen Vorwurfs";

/// Presentation only. The owner and the email admission gate decide whether a
/// message may be sent. Do not interpret free-text automation as actor evidence.
pub(super) fn compose(
    message: &Value,
    sender: Option<EmailAddressWithName>,
    recipient: EmailAddressWithName,
) -> Email {
    let statement = &message["body"];
    let context = &message["email_context"];
    let kind = text(statement, "target_kind")
        .or_else(|| text(context, "target_kind"))
        .unwrap_or("");
    let notifier = message["audience"] == "notifier";
    let outcome = text(statement, "outcome").unwrap_or("");
    // This optional name is supplied by the current verified account lookup at
    // admission, never by a contact display name or a sender-controlled body.
    let greeting = text(message, "recipient_name")
        .filter(|name| name.chars().count() <= 80 && !name.chars().any(char::is_control))
        .map_or_else(|| "Hallo,".to_owned(), |name| format!("Hey {name},"));
    let mut paragraphs = vec![greeting];
    let subject;

    if message["audience"] == "recovery" {
        subject = "Dein Zugangslink".to_owned();
        if let Some(link) = text(statement, "recovery_link") {
            // Preserve the exact case-scoped capability, including its fragment.
            paragraphs.push(format!("hier ist dein angeforderter Zugangslink:\n{link}"));
        }
        let expiry = text(statement, "expires_at")
            .and_then(readable_date)
            .map_or_else(
                || "30 Minuten nach der Anforderung".into(),
                |date| format!("bis {date}"),
            );
        paragraphs.push(format!(
            "Der Link gilt {expiry}. Wenn du ihn nicht angefordert hast, musst du nichts tun."
        ));
    } else if statement["status"] == "received" {
        subject = "Deine Meldung ist angekommen".into();
        paragraphs.push(format!(
            "deine Meldung{} ist angekommen.",
            reported_context(kind, context)
        ));
        paragraphs.push(format!("Deine Meldung ansehen:\n{INBOX}"));
    } else if statement["status"] == "complaint_received" {
        subject = "Deine Beschwerde ist angekommen".into();
        paragraphs.push(format!(
            "deine Beschwerde{} ist angekommen.",
            reported_context(kind, context)
        ));
        paragraphs.push(format!(
            "Den Stand deiner Beschwerde findest du hier:\n{INBOX}"
        ));
    } else {
        subject = if notifier {
            "Eine Entscheidung zu deiner Meldung".into()
        } else {
            format!("{}: {}", subject_topic(kind), outcome_label(outcome))
        };
        paragraphs.push(decision_change(statement, kind, context, notifier));

        // Shorten only this exact native automatic-expiry wording. Original
        // statements and any individual reason remain unchanged in the inbox.
        let automatic_expiry = context["decision_automatic"] == true
            && matches!(outcome, "restore" | "authority_end")
            && text(statement, "rationale") == Some(AUTOMATIC_EXPIRY_REASON);
        if let Some(reason) = text(statement, "rationale").or_else(|| text(statement, "text")) {
            paragraphs.push(if automatic_expiry {
                "Die festgelegte Dauer ist abgelaufen.".to_owned()
            } else {
                reason.to_owned()
            });
        }
        if let Some(ground) = text(statement, "ground").filter(|ground| {
            Some(*ground) != text(statement, "rationale")
                && !(automatic_expiry && *ground == AUTOMATIC_EXPIRY_GROUND)
        }) {
            paragraphs.push(format!("Grundlage: {ground}"));
        }
        if !matches!(outcome, "restore" | "authority_end" | "warn")
            && statement["historical_only"] != true
            && let Some(end) = text(statement, "ends_at").and_then(readable_date)
        {
            paragraphs.push(format!("Diese Einschränkung gilt bis {end}."));
        }
        // Only a recorded human review justifies presenting an assessment as
        // such; a future review or a filled free-text field alone does not.
        if statement["human_review"] == true
            && let Some(assessment) = text(statement, "review_assessment")
        {
            paragraphs.push(format!("Ergebnis der Überprüfung: {assessment}"));
        }
        paragraphs.push(format!("Details und kostenlose Überprüfung:\n{INBOX}"));
        // An authority decision may carry a specific appeal route or deadline.
        // Routine receipts above deliberately omit the old legal boilerplate.
        if outcome.starts_with("authority_")
            && let Some(redress) = text(statement, "redress")
        {
            paragraphs.push(redress.to_owned());
        }
    }

    // A notifier may have no account; a restricted account still has the
    // separate access route. Its existing form needs these two reference fields.
    if message["audience"] != "recovery" && (notifier || kind == "account") {
        let area = if message["source"] == "challenges" {
            "Inhalte"
        } else {
            "Konto"
        };
        if let Some(case) =
            text(message, "case_id").filter(|case| uuid::Uuid::parse_str(case).is_ok())
        {
            paragraphs.push(format!(
                "Ohne Anmeldung: {ACCESS}\nZugangsdaten: Bereich {area}, Referenz {case}."
            ));
        }
    }
    paragraphs.push("Viele Grüße\nDein Bootstrap Academy Team".into());

    Email {
        sender,
        recipient,
        subject: format!("Bootstrap Academy: {subject}"),
        body: paragraphs.join("\n\n"),
        message_id: Some(format!(
            "moderation-{}-{}@bootstrap.academy",
            message["source"].as_str().unwrap_or_default(),
            message["id"].as_str().unwrap_or_default()
        )),
        content_type: ContentType::Text,
        reply_to: None,
        attachments: vec![],
    }
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value[key]
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn readable_date(value: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| {
            // The explicit zone avoids inventing the recipient's location or DST.
            date.with_timezone(&chrono::Utc)
                .format("%d.%m.%Y um %H:%M Uhr (UTC)")
                .to_string()
        })
}

fn title(context: &Value) -> Option<String> {
    text(context, "target_title").map(|title| {
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut short: String = title.chars().take(160).collect();
        if title.chars().count() > 160 {
            short.push('…');
        }
        format!(" „{short}“")
    })
}

fn reported_context(kind: &str, context: &Value) -> String {
    match kind {
        "subtask" => title(context).map_or_else(
            || " zu einer Aufgabe".into(),
            |title| format!(" zu einer Aufgabe aus{title}"),
        ),
        "account" => " zum Kontozugang".into(),
        "create" => " zum Erstellen von Aufgaben".into(),
        "report" => " zum Melden von Aufgaben".into(),
        _ => String::new(),
    }
}

fn topic(kind: &str, context: &Value, notifier: bool, accusative: bool) -> String {
    match kind {
        "subtask" => format!(
            "{} Aufgabe{}",
            if notifier {
                "die von dir gemeldete"
            } else {
                "deine"
            },
            title(context).map_or_else(String::new, |title| format!(" aus{title}"))
        ),
        "account" => match (notifier, accusative) {
            (true, false) => "der von dir gemeldete Kontozugang",
            (true, true) => "den von dir gemeldeten Kontozugang",
            (false, false) => "dein Kontozugang",
            (false, true) => "deinen Kontozugang",
        }
        .into(),
        "create" => format!(
            "das Erstellen von Aufgaben mit {}",
            if notifier {
                "dem gemeldeten Konto"
            } else {
                "deinem Konto"
            }
        ),
        "report" => format!(
            "das Melden von Aufgaben mit {}",
            if notifier {
                "dem gemeldeten Konto"
            } else {
                "deinem Konto"
            }
        ),
        _ => if notifier {
            "deine Meldung"
        } else {
            "deinen Vorgang"
        }
        .into(),
    }
}

fn subject_topic(kind: &str) -> &str {
    match kind {
        "subtask" => "Deine Aufgabe",
        "account" => "Dein Kontozugang",
        "create" => "Aufgaben erstellen",
        "report" => "Aufgaben melden",
        _ => "Dein Vorgang",
    }
}

fn outcome_label(outcome: &str) -> &str {
    match outcome {
        "provisional" => "vorläufig ausgeblendet",
        "remove" => "entfernt",
        "retire" => "aus dem aktiven Angebot genommen",
        "restrict" => "eingeschränkt",
        "restore" => "Einschränkung beendet",
        "warn" => "Warnung",
        "uphold" => "bisherige Entscheidung bestätigt",
        "authority_start" => "behördliche Anordnung umgesetzt",
        "authority_change" => "behördliche Anordnung geändert",
        "authority_end" => "behördliche Einschränkung beendet",
        _ => "neue Entscheidung",
    }
}

fn decision_change(statement: &Value, kind: &str, context: &Value, notifier: bool) -> String {
    if kind.is_empty() {
        return if notifier {
            "zu deiner Meldung gibt es eine Entscheidung."
        } else {
            "zu deinem Vorgang gibt es eine Entscheidung."
        }
        .into();
    }
    let subject = topic(kind, context, notifier, false);
    let object = topic(kind, context, notifier, true);
    match statement["outcome"].as_str().unwrap_or("") {
        "provisional" => format!(
            "{subject} wurde {}vorläufig ausgeblendet.",
            if context["decision_automatic"] == true {
                "automatisch "
            } else {
                ""
            }
        ),
        "remove" => format!("{subject} wurde entfernt."),
        "retire" => format!("{subject} wurde aus dem aktiven Angebot genommen."),
        "restrict" => format!("{subject} wurde eingeschränkt."),
        "restore" => format!("die Einschränkung für {object} aus diesem Vorgang ist beendet."),
        "warn" => format!("für {object} gibt es eine Warnung."),
        "uphold" if statement["historical_only"] == true => format!(
            "für {object} wurde die frühere Entscheidung bestätigt. Dadurch entsteht keine neue Einschränkung."
        ),
        "uphold" => format!("für {object} bleibt die bisherige Entscheidung bestehen."),
        "authority_start" => {
            format!("für {object} wurde eine behördlich angeordnete Einschränkung umgesetzt.")
        }
        "authority_change" => {
            format!("für {object} wurde eine behördlich angeordnete Einschränkung geändert.")
        }
        "authority_end" => {
            format!("die behördliche Einschränkung für {object} aus diesem Vorgang ist beendet.")
        }
        _ => format!("für {object} gibt es eine neue Entscheidung."),
    }
}

#[cfg(test)]
mod tests;
