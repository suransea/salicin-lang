# Salicin

Salicin is an experimental, statically compiled language with an LLVM backend. Its core model
combines uniform `let` declarations, curried compile-time and runtime application, deterministic
ownership, static traits, pattern matching, closures, and explicit algebraic effects. Source files
use `.sc`; the compiler executable is `salic`.

> Salicin is under active development. Its syntax, semantics, and standard library are not stable.

```sc check
let add(x: i32)(y: i32): i32 = { x + y }

let main(): i32 = {
  let add_two = add(2)
  add_two(40)
}
```

## Build and run

The compiler requires Rust. Building or running a native executable also requires `clang` on
`PATH`.

```sh
cargo build --release
target/release/salic run examples/basics.sc
```

`examples/inventory` is the main library acceptance example. It combines modules, owning UTF-8
strings, vectors, results, user traits, resource transfer, iteration, and cleanup.

Common commands:

```sh
salic check main.sc
salic emit-ir main.sc -o main.ll
salic fingerprint main.sc
salic build main.sc -o main
salic run main.sc -- argument
salic test main.sc
salic test main.sc --list
salic test main.sc --filter arithmetic
```

Tests use compile-time registrations and are linked into one runner:

```sc fragment
test("arithmetic") {
  20 + 22 == 42
}
```

Project builds use `salicin.toml`, discover `src/lib.sc` and `src/main.sc`, and
place artifacts under `build/`. A root `[workspace]` lists package members;
`--package` selects a member, and the workspace shares its root `build`
directory and `salicin.lock`. Local path dependencies remain explicit.
`--locked` requires the recorded graph to remain current; `--frozen` also
forbids dependency network access.

## Repository layout

```text
compiler/   Rust implementation of salic
library/    Salicin core and allocation libraries
runtime/    Minimal native runtime support
docs/       Language, compiler, library, runtime, and project documentation
examples/   Small Salicin programs
tests/      End-to-end compiler tests
```

Documentation starts at [docs/README.md](docs/README.md):

- [language specification](docs/language/specification.md)
- [grammar](docs/language/grammar.md)
- [compiler architecture](docs/compiler/architecture.md)
- [standard library](docs/standard-library/README.md)
- [implementation status](docs/project/status.md)
- [project roadmap](docs/project/roadmap.md)
- [project TODO](docs/project/todo.md)

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Salicin is licensed under the [MIT License](LICENSE).
