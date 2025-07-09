--: CourseUser()

--! get_course_user : CourseUser
select * from course_users where course_id=:course_id and user_id=:user_id;

--! update_course_user (purchased?)
insert into course_users as cu (course_id, user_id, purchased)
  values (:course_id, :user_id, coalesce(:purchased, false))
  on conflict (course_id, user_id) do update
    set purchased = coalesce(:purchased, cu.purchased);
