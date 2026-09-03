create type contract_declaration_kind as enum ('cancellation', 'withdrawal');
create type contract_declaration_contract as enum ('premium', 'coins', 'other');
create type contract_cancellation_type as enum ('ordinary', 'extraordinary');

create table contract_declarations (
    id uuid primary key,
    kind contract_declaration_kind not null,
    received_at timestamp with time zone not null,
    name text not null,
    email text not null,
    user_id uuid references users(id) on delete set null,
    contract contract_declaration_contract not null,
    cancellation_type contract_cancellation_type,
    details text not null,
    requested_end timestamp with time zone,
    effective_end timestamp with time zone,
    processed_at timestamp with time zone
);

create index contract_declarations_received_at_idx on contract_declarations (received_at desc);
