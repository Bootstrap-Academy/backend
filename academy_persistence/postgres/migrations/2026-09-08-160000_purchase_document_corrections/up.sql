-- Document interpretations are additive. Original words, operation timestamps,
-- observations, rights and financial/staff decisions are never rewritten.
ALTER TABLE purchase_fulfillments ADD COLUMN statement_version integer NOT NULL DEFAULT 1;
ALTER TABLE purchase_provision_observations ADD COLUMN statement_version integer NOT NULL DEFAULT 1;
CREATE TABLE purchase_document_corrections (
 order_id uuid NOT NULL REFERENCES purchase_offers(id),
 document_kind text NOT NULL CHECK(document_kind IN ('fulfillment','timing')),
 version integer NOT NULL CHECK(version=1),
 original_sha256 text NOT NULL,
 created_at timestamptz NOT NULL,
 evidence jsonb NOT NULL,
 statement text NOT NULL,
 statement_sha256 text NOT NULL CHECK(statement_sha256=encode(sha256(convert_to(statement,'UTF8')),'hex')),
 PRIMARY KEY(order_id,document_kind,version)
);
CREATE TRIGGER immutable_purchase_document_correction BEFORE UPDATE ON purchase_document_corrections
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();

CREATE FUNCTION correct_purchase_document(order_uuid uuid, kind text, original text, facts jsonb) RETURNS void LANGUAGE plpgsql AS $$
DECLARE
 original_hash text := encode(sha256(convert_to(original,'UTF8')),'hex');
 corrected_at timestamptz := clock_timestamp();
 detail text;
 corrected_statement text;
 booking_proof boolean;
BEGIN
 SELECT o.source='events' AND o.offer->'product'->'facts'->>'availability_protocol'='committed_candidate_v1'
   AND f.result->>'timing_basis'='committed_candidate_v1'
 INTO booking_proof FROM purchase_offers o LEFT JOIN purchase_fulfillments f ON f.order_id=o.id WHERE o.id=order_uuid;
 detail := CASE WHEN kind='fulfillment' THEN
   'Eine frühere Angabe „bereitgestellt am“ ist bei Kursfreischaltungen und MorphCoins-Gutschriften durch den dort genannten Bearbeitungszeitpunkt nicht belegt. Dieser wurde vor Abschluss der Buchung erfasst und weist nicht nach, ab wann Zugang oder Guthaben erstmals nutzbar waren. Zugeordnete Premium-Zeiträume und gebuchte Herz- oder Coin-Mengen bleiben unverändert; auch sie belegen keinen genauen Beginn der technischen Nutzbarkeit.'
 ELSE
   'Die ursprüngliche Zeitprüfung beobachtet einen gespeicherten Vorgang. Eine dort bejahte Fristprüfung belegt allein noch nicht, dass sämtliche Zugangskontrollen die gebuchten Rechte bereits beachteten. Sie darf nicht als Nachweis eines genauen Beginns der tatsächlichen Nutzbarkeit verstanden werden.' END;
 IF booking_proof THEN
   detail := detail || E'\nDer getrennt erhaltene Nachweis der Buchungsplattform über beobachtete verfügbare Zugangsdaten bleibt bestehen. Er beschreibt eine Beobachtungsgrenze, nicht den exakten ersten Zugang oder die tatsächliche Durchführung des Termins.';
 ELSE
   detail := detail || E'\nAuch ein historischer Buchungs- oder Meldungszeitpunkt ist ohne einen passenden Nachweis verfügbarer Zugangsdaten kein belegter Beginn der Nutzbarkeit.';
 END IF;
 corrected_statement := format(E'Korrektur und Einordnung zum ursprünglichen Nachweis – Bootstrap Academy\nBestellung: %s\nDokument: %s; Korrekturfassung: 1\nErstellt am %s (UTC). Dies ist das Datum dieser Einordnung, keine Leistungszeit.\nUrsprünglicher Nachweis, SHA-256: %s\n%s\nEin genauer früherer Beginn der Nutzbarkeit wird mit dieser Korrektur nicht festgestellt. Spätere Beobachtungen werden nicht rückdatiert. Originalbelege, vorhandene Zugangsmöglichkeiten und Guthaben, Vertragsbedingungen, gesetzliche Rechte und offene Ansprüche bleiben erhalten; hiermit wird weder eine Erstattung entschieden noch eine Leistung rückwirkend aufgehoben.\nDer ursprüngliche Wortlaut bleibt als historisches Original getrennt abrufbar. Seine Zeitangaben sind zusammen mit dieser Korrektur zu lesen.\n',order_uuid,kind,corrected_at AT TIME ZONE 'UTC',original_hash,detail);
 INSERT INTO purchase_document_corrections(order_id,document_kind,version,original_sha256,created_at,evidence,statement,statement_sha256)
 VALUES(order_uuid,kind,1,original_hash,corrected_at,facts,corrected_statement,encode(sha256(convert_to(corrected_statement,'UTF8')),'hex'))
 ON CONFLICT DO NOTHING;
END $$;
CREATE FUNCTION correct_legacy_purchase_document() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.statement_version=1 THEN
   IF TG_TABLE_NAME='purchase_fulfillments' THEN
     PERFORM correct_purchase_document(NEW.order_id,'fulfillment',NEW.statement,NEW.result);
   ELSE
     PERFORM correct_purchase_document(NEW.order_id,'timing',NEW.statement,NEW.evidence);
   END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER correct_legacy_purchase_fulfillment AFTER INSERT ON purchase_fulfillments
 FOR EACH ROW EXECUTE FUNCTION correct_legacy_purchase_document();
CREATE TRIGGER correct_legacy_purchase_timing AFTER INSERT ON purchase_provision_observations
 FOR EACH ROW EXECUTE FUNCTION correct_legacy_purchase_document();
SELECT correct_purchase_document(order_id,'fulfillment',statement,result) FROM purchase_fulfillments;
SELECT correct_purchase_document(order_id,'timing',statement,evidence) FROM purchase_provision_observations;
