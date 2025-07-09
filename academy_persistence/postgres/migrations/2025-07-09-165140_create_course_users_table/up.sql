create table course_users (
    course_id text not null,
    user_id uuid not null references users(id) on delete cascade,
    purchased boolean not null,
    primary key (course_id, user_id)
);
