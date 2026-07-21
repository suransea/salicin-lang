# Salicin

Salicin is an experimental, statically compiled programming language with an LLVM backend. It
explores uniform `let` declarations, curried parameter groups, ownership-aware argument passing,
traits, pattern matching, closures, and source-backed language items. Source files use `.sali`;
the compiler executable is `salic`.

> Salicin is under active development. Its syntax, semantics, and standard library are not stable.

```sali
let add(x: i32)(y: i32): i32 = x + y

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
target/release/salic run examples/basics.sali
```

Common commands:

```sh
salic check main.sali
salic emit-ir main.sali -o main.ll
salic build main.sali -o main
salic run main.sali -- argument
```

Project builds use `salicin.toml`, discover `src/lib.sali` and `src/main.sali`, and place artifacts
under `build/`. Local path dependencies are recorded in `salicin.lock`.

## Repository layout

```text
compiler/   Rust implementation of salic
library/    Salicin core and allocation libraries
runtime/    Minimal native runtime support
docs/       Language, compiler, library, runtime, and project documentation
examples/   Small Salicin programs
tests/      End-to-end compiler tests
```

Documentation starts at [docs/README.md](docs/README.md). In particular:

- [language specification](docs/language/specification.md)
- [grammar](docs/language/grammar.md)
- [compiler architecture](docs/compiler/architecture.md)
- [standard-library organization](docs/standard-library/README.md)
- [implementation status](docs/project/status.md)
- [release history](CHANGELOG.md)

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Salicin is licensed under the [MIT License](LICENSE).
