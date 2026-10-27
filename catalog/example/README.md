# Library template

Copy this folder to start your own Globw library and publish it with `gwget`.

A library is a folder with a `gw.json` manifest and one or more `.gw` files.
The entry module must be `<name>.gw`, where `<name>` is the `name` in `gw.json`.

## Steps

1. **Rename** `example.gw` to `<yourname>.gw` (lower-case letters, digits, `_`).
2. **Edit `gw.json`**: set `name` (same as the file), `version`, `description`
   and `author` (optional, can stay `anonymous`).
3. **Write your code** in the `.gw` file(s).
4. **Publish**, from inside the folder:

   ```sh
   gwget publish
   ```

It goes online immediately. The **first** publish prints a one-time **edit
token** — keep it: it is the only way to update the library later, and it
protects your library's name.

## Using it

Once published, anyone can install and import it:

```sh
gwget install <yourname>
```

```gw
import <yourname>
output(<yourname>.greeting("Globw"))
```

The catalogue rejects spam, links and offensive text in the name, description
and author, and code that is an exact copy of an existing library.
