-- Globw library catalogue -- Supabase schema.
-- Run this once in the Supabase SQL editor (it is safe to run again).
--
-- Model: anyone may publish or update a library instantly and anonymously; a
-- secret edit token (chosen by gwget on the first publish) protects the name
-- afterwards. Readers see only non-blocked libraries and never the token hash.
-- The maintainer moderates from the dashboard: set `blocked = true` to take a
-- library offline, or delete the row.

create extension if not exists pgcrypto;

create table if not exists public.libraries (
  name            text primary key,
  version         text not null,
  description     text not null default '',
  author          text not null default 'anonymous',
  files           jsonb not null,            -- { "<name>.gw": "code", ... }
  edit_token_hash text not null,             -- sha256 hex of the author's token
  blocked         boolean not null default false,
  created_at      timestamptz not null default now(),
  updated_at      timestamptz not null default now()
);

-- What readers are allowed to see: non-blocked rows, without the token hash.
create or replace view public.libraries_public as
  select name, version, description, author, files, updated_at
  from public.libraries
  where blocked = false;

-- Lock the table itself: the public (anon) key cannot touch it directly.
alter table public.libraries enable row level security;
revoke all on public.libraries from anon, authenticated;

-- Publish or update a library in one step, checking the edit token server-side.
create or replace function public.publish_library(
  p_name text, p_version text, p_description text, p_author text,
  p_files jsonb, p_token text
) returns text
language plpgsql
security definer
set search_path = public
as $$
declare existing text;
begin
  if p_name !~ '^[a-z][a-z0-9_]{0,39}$' then
    raise exception 'invalid library name "%": lower-case letters, digits and _ only', p_name;
  end if;
  if p_files is null or jsonb_typeof(p_files) <> 'object' or p_files = '{}'::jsonb then
    raise exception 'the library has no files';
  end if;
  if coalesce(length(p_token), 0) < 8 then
    raise exception 'missing edit token';
  end if;

  select edit_token_hash into existing from public.libraries where name = p_name;
  if existing is null then
    insert into public.libraries(name, version, description, author, files, edit_token_hash)
    values (p_name, p_version, coalesce(p_description, ''), coalesce(p_author, 'anonymous'),
            p_files, encode(digest(p_token, 'sha256'), 'hex'));
    return 'published';
  elsif existing = encode(digest(p_token, 'sha256'), 'hex') then
    update public.libraries
       set version = p_version, description = coalesce(p_description, ''),
           author = coalesce(p_author, 'anonymous'), files = p_files, updated_at = now()
     where name = p_name;
    return 'updated';
  else
    raise exception 'the name "%" is taken and the edit token is wrong', p_name;
  end if;
end;
$$;

-- The public key may read the safe view and call publish; nothing else.
grant select on public.libraries_public to anon, authenticated;
grant execute on function public.publish_library(text, text, text, text, jsonb, text) to anon, authenticated;
