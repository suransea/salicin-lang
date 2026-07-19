# Salicin

Salicin 是一个基于 LLVM 的静态编译语言实验。源码使用 `.sali` 后缀，编译器命令为 `salic`。
语言设计稿见 [LANGUAGE.md](LANGUAGE.md)，语法骨架见 [GRAMMAR.md](GRAMMAR.md)。

## 构建编译器

```sh
cargo build --release
```

生成的编译器位于 `target/release/salic`。生成本机可执行文件时，`salic` 需要能够从 `PATH` 找到
`clang`；`check` 和 `emit-ir` 不需要链接器。

## 使用

```sh
salic build main.sali -o main       # 编译并链接本机程序
salic check main.sali               # 只做语法和类型检查
salic emit-ir main.sali             # 将 LLVM IR 输出到 stdout
salic emit-ir main.sali -o main.ll  # 将 LLVM IR 写入文件
salic run main.sali                 # 临时编译并运行
salic main.sali -o main             # build 的单文件简写
```

`build` 未指定 `-o` 时，默认输出为去掉 `.sali` 后缀的源码路径。`run` 可用 `--` 分隔并传递程序
参数，例如 `salic run main.sali -- arg1`。

## M0 能力

首个可运行版本支持：

- 单文件编译，以及 `i32`、`i64`、`u32`、`u64`、`bool` 和 `()`；
- 顶层常量与非泛型具名函数；
- 多参数组函数的完整调用；
- 局部 `let`、`let mut` 和赋值；
- 算术、比较、逻辑表达式，以及 `do`、`if` 和 `return`；
- 生成 LLVM IR，并通过 `clang` 链接本机程序。

最小示例：

```sali
let add(x: i32)(y: i32): i32 = x + y

let main(): i32 = add(1)(41)
```

parser 可能已经接受部分闭包和尾随闭包语法，但当前后端不承诺支持它们。结构体、trait、所有权与
借用、部分应用、`match`、错误传播和异步仍属于 M1 及后续版本，尚未实现。
