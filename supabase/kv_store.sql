-- Schema for the Supabase key-value store (SUPABASE_TABLE, default `kv_store`).
-- Run once in the Supabase SQL editor (or via `supabase db push`).

create table if not exists public.kv_store (
  key        text primary key check (char_length(key) between 1 and 256),
  value      jsonb       not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

-- Lock the table down: with RLS enabled and no policies, the public `anon` and
-- `authenticated` roles cannot read or write it. The server uses the secret
-- `service_role` key (SUPABASE_KEY), which bypasses RLS. Never expose that key
-- to browsers.
alter table public.kv_store enable row level security;
