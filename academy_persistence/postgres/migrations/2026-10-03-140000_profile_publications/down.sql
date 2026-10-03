DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM profile_publication_state WHERE policy_active) THEN
  RAISE EXCEPTION 'Activated privacy policy must survive recovery; down migration refused';
 END IF;
END $$;
DROP TRIGGER publication_account_epoch ON users;
DROP TRIGGER publication_hold_epoch ON moderation_holds;
DROP TRIGGER publication_target_epoch ON moderation_targets;
DROP TRIGGER publication_profile_epoch ON user_profiles;
DROP TRIGGER publication_profile_guard ON user_profiles;
DROP TRIGGER publication_activate ON profile_publication_state;
DROP TRIGGER publication_policy_guard ON profile_publication_state;
DROP FUNCTION profile_publication_account_epoch();
DROP FUNCTION profile_publication_moderation_epoch();
DROP FUNCTION profile_publication_recheck(text,text);
DROP FUNCTION profile_publication_profile_epoch();
DROP FUNCTION profile_publication_profile_guard();
DROP FUNCTION profile_publication_activate();
DROP FUNCTION profile_publication_policy_guard();
DROP FUNCTION profile_publication_touch();
DROP TABLE profile_publication_state;
ALTER TABLE user_profiles DROP CONSTRAINT profile_publication_receipt_required,
 DROP COLUMN profile_visibility,DROP COLUMN visibility_revision,DROP COLUMN shared_scope_version,
 DROP COLUMN shared_notice_hash,DROP COLUMN shared_at,DROP COLUMN withdrawn_at,
 DROP COLUMN last_shared_receipt,DROP COLUMN last_private_receipt;
