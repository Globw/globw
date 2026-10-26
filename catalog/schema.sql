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
  files_hash      text,                      -- sha256 of files, to reject copies
  edit_token_hash text not null,             -- sha256 hex of the author's token
  blocked         boolean not null default false,
  created_at      timestamptz not null default now(),
  updated_at      timestamptz not null default now()
);

-- for catalogues created before the anti-copy check existed
alter table public.libraries add column if not exists files_hash text;

-- A hash of a library's file CONTENTS (sorted, names ignored), so a copy
-- republished under a different name still hashes the same.
create or replace function public.gw_files_hash(p_files jsonb) returns text
language sql immutable
set search_path = public, extensions
as $$
  select encode(digest(coalesce(string_agg(value, chr(10) order by value), ''), 'sha256'), 'hex')
  from jsonb_each_text(p_files);
$$;

update public.libraries set files_hash = public.gw_files_hash(files);

-- Rate-limit log: one row per publish attempt, keyed by a HASH of the client
-- IP (no raw IPs stored). Only publish_library (security definer) writes it.
create table if not exists public.publish_log (
  id      bigint generated always as identity primary key,
  ip_hash text not null,
  name    text,
  at      timestamptz not null default now()
);
create index if not exists publish_log_ip_at on public.publish_log(ip_hash, at);
alter table public.publish_log enable row level security;
revoke all on public.publish_log from anon, authenticated;

-- True when a name / description / author looks like spam, links or offensive
-- text. The maintainer's block button is the backstop; this stops the obvious.
create or replace function public.gw_blocked_text(txt text) returns boolean
language plpgsql immutable
as $$
declare
  t  text := lower(coalesce(txt, ''));
  sq text := regexp_replace(lower(coalesce(txt, '')), '[^a-z0-9]', '', 'g');
begin
  if t ~ '(https?://|www\.|\.com|\.net|\.io|\.xyz|t\.me|@[a-z0-9_]{3,}|telegram|whatsapp|casino|crypto|bitcoin|viagra|buy now|click here|free money|\y(promo|sponsor|discount|coupon)\y)' then
    return true;
  end if;
  if t ~ '\y(fuck|shit|bitch|asshole|cunt|pussy|bastard|slut|whore|nigger|nigga|faggot|retard|chink|spic|kike|tranny|cazzo|merda|stronzo|troia|puttana|vaffanculo|coglione|negro|frocio|terrone)\y' then
    return true;
  end if;
  if sq ~ '(porn|nigger|faggot|rape)' then
    return true;
  end if;
  return false;
end;
$$;

-- What readers may see: non-blocked rows, safe columns only. The view runs as
-- the CALLER (security_invoker), so it respects the base table's RLS below
-- instead of the view creator's elevated rights (Supabase advisor-clean).
create or replace view public.libraries_public
  with (security_invoker = true) as
  select name, version, description, author, files, updated_at
  from public.libraries
  where blocked = false;

-- Base table: RLS on. The public (anon) key may read ONLY non-blocked rows and
-- ONLY the safe columns (never edit_token_hash / blocked / files_hash), and can
-- never insert/update/delete directly -- that only goes through publish_library.
alter table public.libraries enable row level security;
revoke all on public.libraries from anon, authenticated;
drop policy if exists libraries_read_public on public.libraries;
create policy libraries_read_public on public.libraries
  for select to anon, authenticated using (blocked = false);
grant select (name, version, description, author, files, updated_at)
  on public.libraries to anon, authenticated;

-- Publish or update a library in one step, checking the edit token server-side.
create or replace function public.publish_library(
  p_name text, p_version text, p_description text, p_author text,
  p_files jsonb, p_token text
) returns text
language plpgsql
security definer
set search_path = public, extensions
as $$
declare
  existing text;
  v_hash   text;
  v_hdrs   json;
  v_ip     text;
  v_iph    text;
begin
  -- rate limiting by client IP (hashed; best-effort anti-flood). Counts every
  -- attempt, so spam and wrong-token hammering are throttled too.
  v_hdrs := nullif(current_setting('request.headers', true), '')::json;
  v_ip := coalesce(v_hdrs ->> 'cf-connecting-ip', v_hdrs ->> 'x-real-ip',
                   nullif(split_part(coalesce(v_hdrs ->> 'x-forwarded-for', ''), ',', 1), ''),
                   'unknown');
  v_iph := encode(digest(v_ip, 'sha256'), 'hex');
  delete from public.publish_log where at < now() - interval '1 day';
  if (select count(*) from public.publish_log where ip_hash = v_iph and at > now() - interval '1 minute') >= 5 then
    raise exception 'too many publishes, slow down (max 5 per minute)';
  end if;
  if (select count(*) from public.publish_log where ip_hash = v_iph and at > now() - interval '1 hour') >= 30 then
    raise exception 'too many publishes, try again later (max 30 per hour)';
  end if;
  insert into public.publish_log(ip_hash, name) values (v_iph, p_name);

  if p_name !~ '^[a-z][a-z0-9_]{0,39}$' then
    raise exception 'invalid library name "%": lower-case letters, digits and _ only', p_name;
  end if;
  if p_files is null or jsonb_typeof(p_files) <> 'object' or p_files = '{}'::jsonb then
    raise exception 'the library has no files';
  end if;
  if coalesce(length(p_token), 0) < 8 then
    raise exception 'missing edit token';
  end if;
  if length(coalesce(p_version, '')) = 0 or length(p_version) > 30 then
    raise exception 'the version must be 1 to 30 characters';
  end if;
  -- size limits: keep metadata short and the payload small (anti-DoS / abuse)
  if length(coalesce(p_description, '')) > 300 then
    raise exception 'the description is too long (max 300 characters)';
  end if;
  if length(coalesce(p_author, '')) > 60 then
    raise exception 'the author is too long (max 60 characters)';
  end if;
  if octet_length(p_files::text) > 262144 then
    raise exception 'the library is too large (max 256 KB)';
  end if;
  if (select count(*) from jsonb_object_keys(p_files)) > 20 then
    raise exception 'too many files (max 20)';
  end if;
  -- the entry module must be present, and every file must be a safe <name>.gw
  if not (p_files ? (p_name || '.gw')) then
    raise exception 'the library must contain %.gw (its entry module)', p_name;
  end if;
  if exists (select 1 from jsonb_object_keys(p_files) as t(k) where t.k !~ '^[a-z0-9_-]+\.gw$') then
    raise exception 'file names must be lower-case letters, digits, _ or -, ending in .gw';
  end if;
  -- anti-spam: reject links, promotions and offensive text in the metadata
  if public.gw_blocked_text(p_name) or public.gw_blocked_text(p_version)
     or public.gw_blocked_text(p_description) or public.gw_blocked_text(p_author) then
    raise exception 'rejected: the metadata looks like spam, a link or offensive content';
  end if;

  v_hash := public.gw_files_hash(p_files);
  -- anti-copy: the same code already published under a different name
  if exists (select 1 from public.libraries where files_hash = v_hash and name <> p_name) then
    raise exception 'rejected: this is a copy of an existing library (identical code)';
  end if;

  select edit_token_hash into existing from public.libraries where name = p_name;
  if existing is null then
    insert into public.libraries(name, version, description, author, files, files_hash, edit_token_hash)
    values (p_name, p_version, coalesce(p_description, ''), coalesce(p_author, 'anonymous'),
            p_files, v_hash, encode(digest(p_token, 'sha256'), 'hex'));
    return 'published';
  elsif existing = encode(digest(p_token, 'sha256'), 'hex') then
    update public.libraries
       set version = p_version, description = coalesce(p_description, ''),
           author = coalesce(p_author, 'anonymous'), files = p_files, files_hash = v_hash, updated_at = now()
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

-- Postgres grants EXECUTE on new functions to PUBLIC by default. Take it back
-- from the internal helpers: only publish_library (security definer) needs
-- them, and it calls them as its owner, so this does not break publishing.
revoke execute on function public.gw_files_hash(jsonb) from public, anon, authenticated;
revoke execute on function public.gw_blocked_text(text) from public, anon, authenticated;
