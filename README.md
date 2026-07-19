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

## 当前能力

v0.1.0 首版支持：

- 单文件编译，以及 `i32`、`i64`、`u32`、`u64`、`bool` 和 `()`；
- 顶层常量与非泛型具名函数；
- 多参数组函数的完整调用；
- 局部 `let`、`let mut` 和赋值；
- 算术、比较、逻辑表达式，以及 `do`、`if` 和 `return`；
- 生成 LLVM IR，并通过 `clang` 链接本机程序。

v0.2.0（M1）在此基础上还支持：

- 名义 `struct`、`enum`，位置或标签构造器，以及结构体字段读取和修改；
- 带 payload binding、guard 和穷尽性检查的 `match`；
- 多参数组函数的本地、非逃逸部分应用；
- place 级 `copy` / `move` 检查、分支状态合流、共享/可变借用，以及借用参数的指针 ABI；
- 直接局部绑定、非逃逸的闭包，支持共享/可变标量捕获和单次移动捕获，并通过 lambda lifting 静态生成代码。
- `while`、`loop` 和带值 `break`，以及循环控制流的保守所有权检查；
- 内联定长 `Array(T, N)`、数组字面量，以及带运行期越界检查的 `i32` 索引。
- 固有 `extend`：支持带 `borrow self`、`mut borrow self` 或 `move self` 的静态分派实例方法，
  以及类型命名空间中的关联函数和关联常量。

v0.3.0（M2，开发中）当前还支持：

- `type` 编译期参数组与显式泛型具名函数调用；
- 带稳定实例缓存的按需单态化，且类型应用后仍可使用既有的局部部分应用；
- 对所有泛型函数体做定义期抽象检查，包括未被调用的模板。
- 泛型 `struct` 与 `enum` 的显式类型应用、嵌套实例、构造器、variant 和模式匹配；
- 结构化的名义类型实例元数据，以及定义期模板校验、递归值布局诊断和实例上限保护。

最小示例：

```sali
let add(x: i32)(y: i32): i32 = x + y

let main(): i32 = add(1)(41)
```

捕获闭包目前采用保守子集：多参数组必须在一次调用链中完整应用，且只支持受限捕获形状和直接调用。
当前借用期采用词法范围。固定数组首版只允许 `Copy` 元素并仅支持只读索引；循环回边禁止移动
循环外部绑定，以保证下一轮仍拥有相同的可用值。方法的临时 receiver 目前需先绑定到局部；bound
method 的部分应用只允许捕获 `Copy` receiver 和实参。表达式路径中的 `Self` 与
`A.method(a)()` 完全限定调用尚未开放。`_` 类型实参推断、泛型或 trait-backed `extend`、错误传播
和异步仍属于后续增量。
