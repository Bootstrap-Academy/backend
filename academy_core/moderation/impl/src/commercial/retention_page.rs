use super::*;

fn exact(value: &Value, keys: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|o| o.len() == keys.len() && keys.iter().all(|key| o.contains_key(*key)))
}

fn keys(family: &str) -> Option<&'static [&'static str]> {
    match family {
        "statements" => Some(&["number"]),
        "archives" => Some(&["number", "kind"]),
        "retained_owner_associations" => Some(&["number", "kind", "subject"]),
        "invoice_identity_reviews" => Some(&["number", "reason", "source_key"]),
        "unqualified_invoice_owner_observations" => {
            Some(&["number", "subject", "basis", "evidence_hash"])
        }
        _ => None,
    }
}

fn canonical_uuid(value: &Value) -> Option<uuid::Uuid> {
    let s = value.as_str()?;
    uuid::Uuid::parse_str(s).ok().filter(|u| u.to_string() == s)
}

fn cursor_shape(value: &Value, family: &str) -> bool {
    let Some(keys) = keys(family) else {
        return false;
    };
    let mut after_keys = vec!["at"];
    after_keys.extend(keys);
    exact(value, &["protocol", "family", "after"])
        && value["protocol"].as_u64() == Some(1)
        && value["family"] == family
        && exact(&value["after"], &after_keys)
        && value["after"]["at"]
            .as_str()
            .is_some_and(|s| !s.is_empty() && !s.contains('\0'))
        && keys.iter().all(|key| {
            if *key == "subject" {
                canonical_uuid(&value["after"][key]).is_some()
            } else {
                // PostgreSQL text/JSONB cannot contain NUL. Reject it before
                // serialization while preserving every other literal key byte.
                value["after"][key]
                    .as_str()
                    .is_some_and(|s| !s.contains('\0'))
            }
        })
}

pub(super) fn validate_body(body: &Value) -> anyhow::Result<()> {
    let valid = exact(body, &["family", "limit", "cursor"])
        && body["limit"]
            .as_u64()
            .is_some_and(|n| (1..=100).contains(&n))
        && body["family"].as_str().is_some_and(|family| {
            keys(family).is_some()
                && (body["cursor"].is_null() || cursor_shape(&body["cursor"], family))
        });
    ensure!(valid, RecipientAccessError::Malformed);
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Timestamp {
    NegativeInfinity,
    Finite(i128),
    Infinity,
}

// Validate the SQL-local ISO,YMD/UTC output without chrono's narrower year range
// or changing any stored text. The integer is only an ordering check.
fn timestamp(s: &str) -> Option<Timestamp> {
    if s == "-infinity" {
        return Some(Timestamp::NegativeInfinity);
    }
    if s == "infinity" {
        return Some(Timestamp::Infinity);
    }
    let (s, bc) = s.strip_suffix(" BC").map_or((s, false), |v| (v, true));
    let (date, time) = s.split_once(' ')?;
    let date: Vec<_> = date.split('-').collect();
    if date.len() != 3 || date[0].len() < 4 || date[1].len() != 2 || date[2].len() != 2 {
        return None;
    }
    fn digits(s: &str) -> Option<i128> {
        (!s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    }
    let displayed_year = digits(date[0])?;
    if !(1..=294_276).contains(&displayed_year) || format!("{displayed_year:04}") != date[0] {
        return None;
    }
    let year = if bc {
        1 - displayed_year
    } else {
        displayed_year
    };
    let month = digits(date[1])?;
    let day = digits(date[2])?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => return None,
    };
    if !(1..=days).contains(&day) {
        return None;
    }
    let time = time.strip_suffix("+00")?;
    let (whole, fraction) = time.split_once('.').map_or((time, ""), |v| v);
    let parts: Vec<_> = whole.split(':').collect();
    if parts.len() != 3 || parts.iter().any(|s| s.len() != 2) {
        return None;
    }
    let hour = digits(parts[0])?;
    let minute = digits(parts[1])?;
    let second = digits(parts[2])?;
    if hour > 23
        || minute > 59
        || second > 59
        || fraction.len() > 6
        || fraction.ends_with('0')
        || (time.contains('.') && fraction.is_empty())
    {
        return None;
    }
    let micros = if fraction.is_empty() {
        0
    } else {
        digits(fraction)? * 10_i128.pow(u32::try_from(6 - fraction.len()).ok()?)
    };
    // Gregorian Julian-day conversion, with a nonnegative shifted year over the
    // entire supported PostgreSQL timestamp range.
    let (y, m) = if month > 2 {
        (year + 4800, month + 1)
    } else {
        (year + 4799, month + 13)
    };
    if y < 0 {
        return None;
    }
    let century = y / 100;
    let julian = y * 365 - 32167 + y / 4 - century + century / 4 + 7834 * m / 256 + day;
    let value = (julian - 2_451_545) * 86_400_000_000
        + (hour * 3600 + minute * 60 + second) * 1_000_000
        + micros;
    (-211_813_488_000_000_000..9_223_371_331_200_000_000)
        .contains(&value)
        .then_some(Timestamp::Finite(value))
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Text(String),
    Uuid(uuid::Uuid),
}

fn position(after: &Value, family: &str) -> Option<(Timestamp, Vec<Key>)> {
    let at = timestamp(after["at"].as_str()?)?;
    let values = keys(family)?
        .iter()
        .map(|key| {
            if *key == "subject" {
                canonical_uuid(&after[key]).map(Key::Uuid)
            } else {
                after[key].as_str().map(|s| Key::Text(s.to_owned()))
            }
        })
        .collect::<Option<Vec<_>>>()?;
    Some((at, values))
}

fn row_after(row: &Value, family: &str) -> Value {
    let date = if matches!(
        family,
        "invoice_identity_reviews" | "unqualified_invoice_owner_observations"
    ) {
        "observed_at"
    } else {
        "review_due_at"
    };
    let mut after = json!({"at":row[date]});
    for key in keys(family).unwrap() {
        after[key] = row[key].clone();
    }
    after
}

fn valid_row(row: &Value, family: &str) -> bool {
    let (fields, dates, nullable_dates, text): (&[&str], &[&str], &[&str], &[&str]) = match family {
        "statements" => (
            &[
                "number",
                "review_due_at",
                "authorized",
                "assessment_json",
                "historical_staff_assertion",
                "issued_at",
            ],
            &["review_due_at", "issued_at"],
            &["historical_staff_assertion"],
            &["number"],
        ),
        "archives" => (
            &[
                "number",
                "kind",
                "source",
                "recorded_at",
                "review_due_at",
                "disposal_authorized",
                "assessment_json",
                "disposal_started_at",
                "file_removed_at",
            ],
            &["recorded_at", "review_due_at"],
            &["disposal_started_at"],
            &["number"],
        ),
        "retained_owner_associations" => (
            &[
                "number",
                "kind",
                "subject",
                "observed_at",
                "review_due_at",
                "source",
            ],
            &["observed_at", "review_due_at"],
            &[],
            &["number", "kind", "source"],
        ),
        "invoice_identity_reviews" => (
            &[
                "number",
                "reason",
                "source_key",
                "observed_at",
                "disposition",
                "evidence_json",
            ],
            &["observed_at"],
            &[],
            &["number", "reason", "source_key", "evidence_json"],
        ),
        "unqualified_invoice_owner_observations" => (
            &[
                "number",
                "subject",
                "basis",
                "evidence_hash",
                "evidence_json",
                "qualified",
                "observed_at",
            ],
            &["observed_at"],
            &[],
            &["number", "basis", "evidence_hash", "evidence_json"],
        ),
        _ => return false,
    };
    exact(row, fields)
        && dates
            .iter()
            .all(|k| row[k].as_str().and_then(timestamp).is_some())
        && nullable_dates
            .iter()
            .all(|k| row[k].is_null() || row[k].as_str().and_then(timestamp).is_some())
        && text.iter().all(|k| row[k].is_string())
        && match family {
            "statements" => {
                row["authorized"].is_boolean()
                    && (row["assessment_json"].is_null() || row["assessment_json"].is_string())
            }
            "archives" => {
                matches!(
                    row["kind"].as_str(),
                    Some("invoice" | "credit_note" | "final_statement")
                ) && matches!(
                    row["source"].as_str(),
                    Some("record_disposal" | "unrecorded_archive")
                ) && row["disposal_authorized"].is_boolean()
                    && (row["assessment_json"].is_null() || row["assessment_json"].is_string())
                    && row["file_removed_at"].is_null()
            }
            "retained_owner_associations" => canonical_uuid(&row["subject"]).is_some(),
            "invoice_identity_reviews" => row["disposition"] == "pending_review",
            "unqualified_invoice_owner_observations" => {
                canonical_uuid(&row["subject"]).is_some() && row["qualified"] == false
            }
            _ => false,
        }
}

pub(super) fn unwrap(value: Value, selected: &Value) -> anyhow::Result<Value> {
    if exact(&value, &["kind"]) && value["kind"] == "malformed" {
        return Err(RecipientAccessError::Malformed.into());
    }
    ensure!(
        exact(&value, &["kind", "value"]) && value["kind"] == "page",
        "Unavailable retention page envelope"
    );
    let page = &value["value"];
    let family = selected["family"].as_str().unwrap();
    let limit = selected["limit"].as_u64().unwrap();
    ensure!(
        exact(
            page,
            &[
                "protocol",
                "family",
                "limit",
                "observed_at",
                "semantics",
                "rows",
                "next_cursor",
                "exhausted"
            ]
        ) && page["protocol"].as_u64() == Some(1)
            && page["family"] == family
            && page["limit"].as_u64() == Some(limit)
            && page["semantics"] == "live_queue"
            && matches!(
                page["observed_at"].as_str().and_then(timestamp),
                Some(Timestamp::Finite(_))
            ),
        "Unavailable retention page projection"
    );
    let rows = page["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Unavailable retention rows"))?;
    let exhausted = page["exhausted"]
        .as_bool()
        .ok_or_else(|| anyhow::anyhow!("Unavailable exhaustion"))?;
    ensure!(
        rows.len() <= usize::try_from(limit)?
            && (exhausted || rows.len() == usize::try_from(limit)?),
        "Unavailable retention page size"
    );
    let mut previous = if selected["cursor"].is_null() {
        None
    } else {
        Some(
            position(&selected["cursor"]["after"], family)
                .ok_or_else(|| anyhow::anyhow!("Unavailable cursor projection"))?,
        )
    };
    for row in rows {
        ensure!(valid_row(row, family), "Unavailable retention row");
        let current = position(&row_after(row, family), family)
            .ok_or_else(|| anyhow::anyhow!("Unavailable retention position"))?;
        ensure!(
            previous.as_ref().is_none_or(|p| p < &current),
            "Unavailable retention row order"
        );
        previous = Some(current);
    }
    let next = &page["next_cursor"];
    if exhausted {
        ensure!(next.is_null(), "Unavailable exhausted cursor");
    } else {
        ensure!(
            cursor_shape(next, family)
                && next["after"]
                    == row_after(
                        rows.last()
                            .ok_or_else(|| anyhow::anyhow!("Unavailable empty continuation"))?,
                        family
                    ),
            "Unavailable next cursor"
        );
    }
    Ok(value["value"].clone())
}
