-- Preparation only: existing leaderboard_opt_out values and account views remain intact.
ALTER TABLE user_profiles
 ADD COLUMN profile_visibility text NOT NULL DEFAULT 'private' CHECK (profile_visibility IN ('private','shared')),
 ADD COLUMN visibility_revision bigint NOT NULL DEFAULT 0 CHECK (visibility_revision >= 0),
 ADD COLUMN shared_scope_version text,
 ADD COLUMN shared_notice_hash text,
 ADD COLUMN shared_at timestamptz,
 ADD COLUMN withdrawn_at timestamptz,
 ADD COLUMN last_shared_receipt jsonb,
 ADD COLUMN last_private_receipt jsonb,
 ADD CONSTRAINT profile_publication_receipt_required CHECK (
  profile_visibility='private' OR coalesce((shared_scope_version IS NOT NULL AND shared_notice_hash IS NOT NULL
   AND shared_at IS NOT NULL AND last_shared_receipt IS NOT NULL
   AND last_shared_receipt->>'profile_visibility'='shared'
   AND last_shared_receipt->>'scope_version'=shared_scope_version
   AND last_shared_receipt->>'notice_hash'=shared_notice_hash
   AND (last_shared_receipt->>'visibility_revision')::bigint=visibility_revision),false));

CREATE TABLE profile_publication_state (
 singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
 policy_active boolean NOT NULL DEFAULT false,
 publication_epoch uuid NOT NULL DEFAULT gen_random_uuid(),
 epoch_revision bigint NOT NULL DEFAULT 0 CHECK (epoch_revision >= 0),
 scope_version text,
 notice_hash text,
 -- Only a policy clock; no user activity or request history.
 moderation_checked_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
INSERT INTO profile_publication_state(singleton) VALUES(true);

CREATE FUNCTION profile_publication_touch() RETURNS void LANGUAGE sql AS $$
 UPDATE profile_publication_state SET publication_epoch=gen_random_uuid(),epoch_revision=epoch_revision+1 WHERE singleton;
$$;

-- The persisted policy is sticky. A runtime flag cannot undo activation.
CREATE FUNCTION profile_publication_policy_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'Publication policy state cannot be deleted'; END IF;
 IF OLD.policy_active AND NOT NEW.policy_active THEN RAISE EXCEPTION 'Activated privacy policy cannot revert to legacy'; END IF;
 IF NOT OLD.policy_active AND NEW.policy_active THEN
  NEW.publication_epoch:=gen_random_uuid(); NEW.epoch_revision:=OLD.epoch_revision+1;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER publication_policy_guard BEFORE UPDATE OR DELETE ON profile_publication_state
 FOR EACH ROW EXECUTE FUNCTION profile_publication_policy_guard();

CREATE FUNCTION profile_publication_activate() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 -- Future coordinated activation: all existing accounts begin private.
 IF NOT OLD.policy_active AND NEW.policy_active THEN
  UPDATE user_profiles SET profile_visibility='private',visibility_revision=visibility_revision+1,
   leaderboard_opt_out=true,shared_scope_version=NULL,shared_notice_hash=NULL,shared_at=NULL,
   withdrawn_at=NULL,last_shared_receipt=NULL,last_private_receipt=NULL;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER publication_activate AFTER UPDATE OF policy_active ON profile_publication_state
 FOR EACH ROW EXECUTE FUNCTION profile_publication_activate();

CREATE FUNCTION profile_publication_profile_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE active boolean;
BEGIN
 SELECT policy_active INTO STRICT active FROM profile_publication_state WHERE singleton;
 IF TG_OP='INSERT' THEN
  -- Account creation/import is never consent, even with an old opt-out value.
  NEW.profile_visibility:='private'; NEW.visibility_revision:=0;
  NEW.shared_scope_version:=NULL; NEW.shared_notice_hash:=NULL; NEW.shared_at:=NULL;
  NEW.withdrawn_at:=NULL; NEW.last_shared_receipt:=NULL; NEW.last_private_receipt:=NULL;
 ELSE
  IF NEW.visibility_revision < OLD.visibility_revision THEN RAISE EXCEPTION 'Publication revision cannot decrease'; END IF;
  IF active AND NEW.leaderboard_opt_out AND OLD.profile_visibility='shared'
    AND NEW.profile_visibility=OLD.profile_visibility THEN
   -- An old client's explicit opt-out may withdraw; false never grants consent.
   NEW.profile_visibility:='private'; NEW.visibility_revision:=OLD.visibility_revision+1;
   NEW.withdrawn_at:=clock_timestamp();
   NEW.last_private_receipt:=jsonb_build_object(
    'request_id',gen_random_uuid(),'expected_revision',OLD.visibility_revision,'visibility_revision',NEW.visibility_revision,
    'profile_visibility','private','recorded_at',floor(extract(epoch FROM NEW.withdrawn_at))::bigint,
    'scope_version',NULL,'notice_hash',NULL,'source','legacy_opt_out');
  END IF;
  IF NEW.profile_visibility IS DISTINCT FROM OLD.profile_visibility AND NEW.visibility_revision <= OLD.visibility_revision THEN
   RAISE EXCEPTION 'Publication transition requires a new revision';
  END IF;
 END IF;
 IF active THEN NEW.leaderboard_opt_out:=(NEW.profile_visibility<>'shared'); END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER publication_profile_guard BEFORE INSERT OR UPDATE ON user_profiles
 FOR EACH ROW EXECUTE FUNCTION profile_publication_profile_guard();

CREATE FUNCTION profile_publication_profile_epoch() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN
  PERFORM profile_publication_touch();
 ELSIF OLD.profile_visibility IS DISTINCT FROM NEW.profile_visibility
    OR OLD.visibility_revision IS DISTINCT FROM NEW.visibility_revision
    OR OLD.shared_scope_version IS DISTINCT FROM NEW.shared_scope_version
    OR OLD.shared_notice_hash IS DISTINCT FROM NEW.shared_notice_hash
    OR (NEW.profile_visibility='shared' AND OLD.display_name IS DISTINCT FROM NEW.display_name) THEN
  PERFORM profile_publication_touch();
 END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER publication_profile_epoch AFTER UPDATE OR DELETE ON user_profiles
 FOR EACH ROW EXECUTE FUNCTION profile_publication_profile_epoch();

CREATE FUNCTION profile_publication_account_epoch() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.enabled IS DISTINCT FROM NEW.enabled OR OLD.email_verified IS DISTINCT FROM NEW.email_verified
  OR OLD.email IS DISTINCT FROM NEW.email THEN PERFORM profile_publication_touch(); END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER publication_account_epoch AFTER UPDATE OF enabled,email_verified,email ON users
 FOR EACH ROW EXECUTE FUNCTION profile_publication_account_epoch();

-- Moderation changes effective enablement through a view, including timed holds.
CREATE FUNCTION profile_publication_moderation_epoch() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE before_kind text; after_kind text;
BEGIN
 IF TG_OP<>'INSERT' THEN before_kind:=coalesce(to_jsonb(OLD)->>'target_kind',to_jsonb(OLD)->>'kind'); END IF;
 IF TG_OP<>'DELETE' THEN after_kind:=coalesce(to_jsonb(NEW)->>'target_kind',to_jsonb(NEW)->>'kind'); END IF;
 IF before_kind='account' OR after_kind='account' THEN PERFORM profile_publication_touch(); END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER publication_hold_epoch AFTER INSERT OR UPDATE OR DELETE ON moderation_holds
 FOR EACH ROW EXECUTE FUNCTION profile_publication_moderation_epoch();
CREATE TRIGGER publication_target_epoch AFTER INSERT OR UPDATE OR DELETE ON moderation_targets
 FOR EACH ROW EXECUTE FUNCTION profile_publication_moderation_epoch();

CREATE FUNCTION profile_publication_recheck(p_scope text,p_notice text) RETURNS void LANGUAGE sql AS $$
 UPDATE profile_publication_state s SET publication_epoch=gen_random_uuid(),epoch_revision=epoch_revision+1,
  moderation_checked_at=clock_timestamp(),scope_version=p_scope,notice_hash=p_notice
 WHERE s.singleton AND (s.scope_version IS DISTINCT FROM p_scope OR s.notice_hash IS DISTINCT FROM p_notice OR
  (s.policy_active AND EXISTS(
  SELECT 1 FROM moderation_holds h WHERE h.target_kind='account' AND h.active AND
   ((h.starts_at>s.moderation_checked_at AND h.starts_at<=clock_timestamp()) OR
    (h.ends_at>s.moderation_checked_at AND h.ends_at<=clock_timestamp())))));
$$;
