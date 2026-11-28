# Globw in terminal editors

Syntax highlighting for `.gw` files in **nano**, **vim** and **micro**, with the same
rules as the VS Code extension: keywords (also `end if`, `end loop`, ...), types,
strings with their escapes, numbers, comments (`'`), and calls.

## Installed by the Linux packages

The `.deb` and `.rpm` packages put the nano and vim files where those editors look
by themselves, so `.gw` files are coloured right after installing Globw:

| Editor | File |
|--------|------|
| nano   | `/usr/share/nano/globw.nanorc` |
| vim    | `/usr/share/vim/vimfiles/{syntax,ftdetect,ftplugin}/globw.vim` |
| micro  | `/usr/share/globw/editors/micro/globw.yaml` (copy it, see below) |

Some systems do not load every file in `/usr/share/nano/`: if nano shows no colours,
add this line to `~/.nanorc`:

```
include "/usr/share/nano/globw.nanorc"
```

## By hand (the `tar.gz`, macOS, or your own user)

The archive has this `editors/` folder next to `gw` and `gwget`.

**nano**

```sh
mkdir -p ~/.nano && cp editors/nano/globw.nanorc ~/.nano/
echo 'include "~/.nano/globw.nanorc"' >> ~/.nanorc
```

**vim**

```sh
mkdir -p ~/.vim && cp -r editors/vim/syntax editors/vim/ftdetect editors/vim/ftplugin ~/.vim/
```

(for Neovim, copy them into `~/.config/nvim/` instead)

**micro**

```sh
mkdir -p ~/.config/micro/syntax && cp editors/micro/globw.yaml ~/.config/micro/syntax/
```
