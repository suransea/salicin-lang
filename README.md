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

salic build                         # 从当前目录向上发现 salicin.toml
salic run ./my-project              # 运行项目的默认二进制 target
salic run salicin.toml --bin tool   # 选择一个 [[bin]] target
salic check --lib                   # 检查项目的 [lib] target
```

`build` 未指定 `-o` 时，默认输出为去掉 `.sali` 后缀的源码路径。`run` 可用 `--` 分隔并传递程序
参数，例如 `salic run main.sali -- arg1`。项目构建默认写入 `build/<target-name>`；项目输入可以是
目录或 `salicin.toml`，省略时会从当前目录逐级向上查找。当前项目清单支持 `[package]`、`[lib]`
和 `[[bin]]`；非 target 的 `src/math.sali`、`src/net/http.sali` 会分别成为 `math`、`net.http`
文件模块。本地依赖与 `use` 将在下一切片开放，现阶段非空 `[dependencies]` 会明确报错。

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

v0.3.0（M2）在此基础上还支持：

- `type` 编译期参数组与显式泛型具名函数调用；
- 带稳定实例缓存的按需单态化，且类型应用后仍可使用既有的局部部分应用；
- 对所有泛型函数体做定义期抽象检查，包括未被调用的模板。
- 泛型 `struct` 与 `enum` 的显式类型应用、嵌套实例、构造器、variant 和模式匹配；
- 结构化的名义类型实例元数据，以及定义期模板校验、递归值布局诊断和实例上限保护。
- 函数、结构体和枚举构造中的顶层 `_` 类型实参推断，可使用期望结果、运行时实参、字段和
  variant payload 约束，并保持实参只求值一次。
- 具体 trait 实现、必需关联类型和唯一候选的静态方法分发；支持普通名义类型与
  `Cell(i32)` 这样的具体泛型实例，并完整校验参数组、传递模式和返回类型。
- 基于 `Add(Rhs)` trait 的名义类型 `+` 运算符，关联类型 `Output` 参与期望类型推断；
  内建整数加法仍直接生成 LLVM 整数指令。
- 预导入的 `Option(T)` 与 `Result(T, E)` 泛型枚举，支持 `Some` / `None` / `Ok` / `Err`
  构造、顶层 `_` 实参推断、嵌套实例和模式匹配。
- 拥有的 `Option(T)` / `Result(T, E)` 上的 `?.` 可选链，支持结构体字段和完整方法调用；成功值
  可从 `T` 变为 `U`，`None` / `Err(E)` 保持原容器形状，方法实参只在成功分支求值，返回的
  容器不会自动展平。
- `Option(T) ?? T` 与 `Result(T, E) ?? T` 的右结合惰性合并；成功分支直接取出 payload，
  只有 `None` / `Err` 分支才求值 fallback。
- `Option(T)` 与 `Result(T, E)` 的后缀 `.try` 传播：操作数只求值一次，成功 payload 继续执行，
  `None` 或同一错误类型的 `Err` 从当前显式返回边界提前返回；该边界中的普通尾值和 `return`
  值会自动包装为 `Some` / `Ok`。
- `throw error` 在显式 `Result(U, E)` 返回边界中把错误值包装成外层 `Err` 并立即返回；错误表达式
  只求值一次，并按边界的精确 `E` 类型检查。

当前 main 分支正在实现 M3，并新增：

- 严格校验的 `salicin.toml`、默认 `src/lib.sali` / `src/main.sali` target 发现、自定义 `[lib]` /
  `[[bin]]`、项目级 target 选择和 `build/` 输出目录；单文件命令保持兼容。
- `void` 作为 `()` 的预导入类型别名，以及作为零 variant enum 预导入的 `never`；空 enum 是
  uninhabited type，可通过空 `match {}` 消除并参与发散控制流的类型统一。
- 自动发现 `src` 下的文件模块，通过两遍名称收集支持跨文件前向引用、嵌套限定路径、结构体与
  trait/extend；子模块声明会降低为稳定的包内 canonical 名称。
- 顶层声明支持默认私有、`pub(package)` 和 `pub`；私有名称可由声明模块及其子模块访问，兄弟
  模块访问会得到可见性诊断。参数、局部 `let`、闭包参数和模式绑定均可正常遮蔽模块名。
- `use` 支持单项、`as`、分组与模块 alias；`root`、`self` 和连续 `super` 可显式选择解析起点，
  `pub use` / `pub(package) use` 可构造 facade，并禁止越权重导出或借私有 alias 绕过可见性。

最小示例：

```sali
let add(x: i32)(y: i32): i32 = x + y

let main(): i32 = add(1)(41)
```

捕获闭包目前采用保守子集：多参数组必须在一次调用链中完整应用，且只支持受限捕获形状和直接调用。
当前借用期采用词法范围。固定数组首版只允许 `Copy` 元素并仅支持只读索引；循环回边禁止移动
循环外部绑定，以保证下一轮仍拥有相同的可用值。方法的临时 receiver 目前需先绑定到局部；bound
method 的部分应用只允许捕获 `Copy` receiver 和实参。表达式路径中的 `Self` 与
`A.method(a)()` 完全限定调用尚未开放。`_` 首版只能占据完整的编译期实参槽；嵌套占位和声明类型
中的占位尚未开放，嵌套的推断式泛型调用以及闭包捕获扫描中的泛型调用仍需显式类型实参。trait 首版
暂不支持 `where`、泛型 impl、默认方法、泛型关联类型、完全限定调用和 trait object；无约束的抽象类型
不能声明为 `copy` 参数。`Add` 暂以顶层 trait 名识别为语言项，重载路径目前要求左操作数能静态
探测为具体名义类型。`Option` / `Result` 构造器目前使用 `Option(i32).Some(1)` 这样的显式类型头；
`??` 首版直接识别这两个预导入容器，尚未开放用户 `Coalesce` 实现。`.try` 首版只在显式标注
`Option` / `Result` 返回类型的具名函数中建立传播边界；`Result` 错误类型必须完全相同，尚未开放
`Try` / residual 转换、传播块或闭包传播。`throw` 复用同一具名 `Result` 边界且暂不执行错误类型
转换。`?.` 首版直接识别拥有的预导入容器，尚未开放用户 `Chain` 实现、借用容器、可变借用
receiver、方法部分应用或可调用字段。其他运算符 trait 和异步也尚未开放。

文件模块当前尚未实现 glob import、公开字段和跨包可见性泄漏检查；private trait 的接收者方法
候选也尚未按调用模块过滤。项目级语义诊断还没有逐 item source map。当前 `pub` 与
`pub(package)` 在同一包内都可见，但为下一步依赖包解析保留不同语义。
