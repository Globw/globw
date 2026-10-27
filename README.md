<p align="center">
  <img src="vscode-globw/images/gw-dark.svg" alt="Globw" width="120">
</p>

<h1 align="center">Globw</h1>

<p align="center">
  A statically typed, interpreted language you write the way you think —
  with graphics, networking, databases, audio, AI, physics and more already
  built in.
</p>

<p align="center">
  <a href="https://github.com/Globw/globw/releases"><img alt="release" src="https://img.shields.io/github/v/release/Globw/globw"></a>
  <a href="LICENSE"><img alt="license" src="https://img.shields.io/badge/license-Apache--2.0-blue"></a>
</p>

---

## What is Globw?

Globw is a small language with a big standard library. You write clear,
readable code — lower-case keywords, `=` to assign and `==` to compare,
1-based indexing, comments with `'` — and it **compiles to bytecode and runs on
the fly** on a fast register VM written in Rust. One file, no runtime to
install, no dependencies to wire up.

```gw
' a tiny program
variables:
    i, total as int
total = 0
for(i from 1 to 5)
    total = total + i
end loop
output("Hello from Globw!")
output("sum 1..5 = " + total)
```

Run it:

```sh
gw hello.gw
```

## Highlights

- **Readable, statically typed** — clear blocks, helpful errors that tell you
  how to fix them.
- **Batteries included** — whole libraries inside the language, no installs:
  GUI, 2D games and physics, audio/DSP, dataframes, autodiff, symbolic math,
  HTTP + a web server, an SQLite database, concurrency, a hosted/local AI
  client, and 1000+ unit conversions.
- **Fast** — a register bytecode VM; the benchmark suite (recursive Fibonacci,
  the sieve of Eratosthenes, a 100M-iteration sum loop) targets at least 5× the
  speed of CPython. See [`bench/`](bench).
- **A real package manager** — [`gwget`](#packages-with-gwget) installs and
  publishes community libraries.
- **Cross-platform** — Windows, macOS and Linux.

## Install

Download the binaries for your system from the
[**latest release**](https://github.com/Globw/globw/releases/latest): it
contains two programs, **`gw`** (the interpreter) and **`gwget`** (the package
manager). Unzip them into a folder and add it to your `PATH`, and you are ready:

```sh
gw program.gw        # run a program
gwget install vec2   # install a library
```

Or build from source (needs [Rust](https://rustup.rs)):

```sh
git clone https://github.com/Globw/globw
cd globw
cargo build --release          # produces target/release/gw and gwget
```

## Running programs

```sh
gw program.gw            # run
gw --check program.gw    # only check for errors, do not run
gw --profile program.gw  # run with an opcode profile
```

## Packages with gwget

`gwget` is Globw's package manager. Installed libraries live in `~/.gw/libs`
and are found automatically by `import`.

```sh
gwget install vec2     # download a library -> then: import vec2
gwget search math      # search the catalogue
gwget list             # what is installed
gwget publish          # publish the library in the current folder
```

Publishing is anonymous and instant: put a `gw.json` manifest and your `.gw`
files in a folder and run `gwget publish`. The first publish prints a one-time
edit token you keep to update the library later. See
[`catalog/`](catalog) for the catalogue setup and
[`catalog/example/`](catalog/example) for a library template.

## Editor support

The [`vscode-globw`](vscode-globw) extension gives `.gw` files syntax
highlighting, a Run button (F5) and live error checking in VS Code.

## Documentation

The language reference is in [`docs/spec.md`](docs/spec.md): structure, types,
expressions, statements, the module system, errors and packages.

## Repository layout

| Path            | What |
|-----------------|------|
| `src/`          | The interpreter (lexer, parser, checker, compiler, VM) and the standard library. |
| `src/bin/`      | The `gwget` package manager. |
| `examples/`     | Example `.gw` programs with their expected output (used by the tests). |
| `bench/`        | Benchmark programs. |
| `docs/`         | The language specification. |
| `catalog/`      | The library catalogue (Supabase schema + setup). |
| `vscode-globw/` | The VS Code extension. |

## Contributing

Issues and pull requests are welcome. Run the tests with `cargo test`.

## License

[Apache-2.0](LICENSE) — © 2026 Globw.
