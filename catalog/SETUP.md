# The Globw library catalogue (`gwget`)

`gwget` installs and publishes external Globw libraries. The catalogue is a
single online table hosted on **Supabase** (free tier). Anyone can publish a
library instantly and anonymously; you (the maintainer) moderate afterwards by
blocking or deleting rows from the Supabase dashboard.

## 1. Create the catalogue (maintainer, once)

1. Make a free project at <https://supabase.com> (choose a region near you).
2. In the project, open **SQL Editor** → **New query**, paste all of
   [`schema.sql`](schema.sql), and **Run**. This creates the `libraries` table,
   the read-only `libraries_public` view and the `publish_library` function.
3. Open **Project Settings → API** and copy two values:
   - **Project URL** — `https://<project>.supabase.co`
   - **anon public** key — a long token. It is *meant* to be public: it only
     allows reading the safe view and calling publish, nothing else.

## 2. Point `gwget` at the catalogue

Either set two environment variables:

```
GWGET_SUPABASE_URL = https://<project>.supabase.co
GWGET_SUPABASE_KEY = <anon public key>
```

…or create `~/.gw/config.json` (on Windows `C:\Users\<you>\.gw\config.json`):

```json
{
  "supabase_url": "https://<project>.supabase.co",
  "supabase_anon_key": "<anon public key>"
}
```

## 3. Use it

```
gwget search math            # find libraries
gwget install vec2           # download into ~/.gw/libs, then: import vec2
gwget remove vec2            # uninstall from this machine
gwget list                   # what is installed here
```

Installed libraries live in `~/.gw/libs` and are found automatically by
`import`. (`GW_HOME` moves the `~/.gw` folder if you want.)

## 4. Publish a library (any author, anonymous)

Put the library in a folder with a manifest `gw.json` and its `.gw` files. The
entry module must be `<name>.gw`:

```
mylib/
  gw.json
  vec2.gw
```

```json
{
  "name": "vec2",
  "version": "1.0.0",
  "description": "2D vectors",
  "author": "anonymous"
}
```

Then, from inside the folder:

```
gwget publish
```

It is online immediately. The **first** publish prints a one-time **edit
token** — keep it: it is the only way to update the library later (it protects
the name from anyone else). `gwget` also saves it in `~/.gw/tokens.json` on the
machine you published from.

## 5. Moderate (maintainer)

Open the Supabase **Table editor → `libraries`**:

- to take a library offline, set its `blocked` column to `true` (it stops being
  served to `gwget install`/`search`, but the row stays so you can review it);
- to remove it for good, delete the row.

Changes take effect immediately. Checking in now and then is enough — nothing
needs pre-approval.

> Note: libraries are code that runs on the machine that installs them. The
> catalogue shows each library's author, and you can block anything, but there
> is no way to detect cleverly hidden malicious code automatically — review is
> the backstop.
