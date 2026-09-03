--: AdminAuditLogEntry (target_user_id?)

--! create (target_user_id?)
insert into admin_audit_log (id, at, admin_user_id, method, path, target_user_id, status, request_id)
  values (:id, :at, :admin_user_id, :method, :path, :target_user_id, :status, :request_id);

--! list (admin_user_id?, target_user_id?) : AdminAuditLogEntry
select * from admin_audit_log
  where (:admin_user_id::uuid is null or admin_user_id = :admin_user_id)
    and (:target_user_id::uuid is null or target_user_id = :target_user_id)
  order by at desc, id desc
  limit :limit offset :offset;

--! count (admin_user_id?, target_user_id?)
select count(*) from admin_audit_log
  where (:admin_user_id::uuid is null or admin_user_id = :admin_user_id)
    and (:target_user_id::uuid is null or target_user_id = :target_user_id);

--! delete_by_at
delete from admin_audit_log where at<:at;
