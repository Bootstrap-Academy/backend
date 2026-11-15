-- The caller holds request, actor, observed account and declaration locks.
SELECT EXISTS (
    SELECT 1
    FROM contract_declarations d
    JOIN contract_account_observation o ON o.declaration_id = d.id
    JOIN users u ON u.id = o.user_id
    WHERE d.id = $1
      AND d.processed_at IS NULL
      AND d.effective_end IS NULL
      AND o.agreement_id IS NULL
      AND NOT EXISTS (SELECT 1 FROM contract_purchase_observations WHERE declaration_id = d.id)
      AND NOT EXISTS (SELECT 1 FROM contract_period_observation WHERE declaration_id = d.id)
      AND NOT EXISTS (SELECT 1 FROM contract_premium_operations WHERE declaration_id = d.id)
      AND NOT EXISTS (SELECT 1 FROM contract_cancellation_schedule WHERE declaration_id = d.id)
      AND NOT EXISTS (SELECT 1 FROM premium WHERE user_id = o.user_id)
      AND NOT EXISTS (SELECT 1 FROM premium_renewal_agreements WHERE user_id = o.user_id)
      AND NOT EXISTS (SELECT 1 FROM premium_subscriptions WHERE user_id = o.user_id)
      AND NOT EXISTS (SELECT 1 FROM financial_documents WHERE user_id = o.user_id)
);
