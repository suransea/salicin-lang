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
目录或 `salicin.toml`，省略时会从当前目录逐级向上查找。项目清单支持 `[package]`、`[lib]`、
`[[bin]]` 和本地路径 `[dependencies]`；非 target 的 `src/math.sali`、`src/net/http.sali` 会分别成为
`math`、`net.http` 文件模块。包命令会确定性更新项目根的 `salicin.lock`。

```toml
[dependencies]
math = { path = "../math" }
```

依赖 alias 是声明它的包内可见的路径首段；传递依赖不会自动暴露，需由中间包用 `pub use` 明确
重导出。当前版本只接受本地 `path`，尚不解析 registry、版本范围或 Git 来源。

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
- 函数、结构体和枚举构造可省略编译期参数组，并使用期望结果、运行时实参、字段和 variant
  payload 约束推断类型实参；命名实参可显式消歧，并保持实参只求值一次。
- 具体 trait 实现、必需关联类型和唯一候选的静态方法分发；支持普通名义类型与
  `Cell(i32)` 这样的具体泛型实例，并完整校验参数组、传递模式和返回类型。
- 基于 `Add(Rhs)` trait 的名义类型 `+` 运算符，关联类型 `Output` 参与期望类型推断；
  内建整数加法仍直接生成 LLVM 整数指令。
- 预导入的 `Option(T)` 与 `Result(T, E)` 泛型枚举，支持 `Some` / `None` / `Ok` / `Err`
  构造、省略编译期组的实参推断、嵌套实例和模式匹配。
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

v0.4.0（M3 的模块与本地包基础）在此基础上还支持：

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
- 严格的本地 `{ path = "..." }` 依赖、规范路径循环检测、仅依赖库 target 的源码发现，以及
  确定性、原子更新的 `salicin.lock`；依赖的其他二进制 target 不参与编译。
- 稳定 `PackageId` 驱动的跨包名称与名义类型身份；`pub(package)` 和私有成员不能越过包边界，
  传递依赖只能经拥有者显式重导出，同一个菱形共享依赖只实例化一次。

v0.5.0 开始引导标准库：

- edition 2026 的首份 `core` 源位于 `library/core/src/prelude.sali`，随编译器一起嵌入，不依赖
  用户机器上的安装路径；它仍通过普通 Salicin lexer、parser 和语义分析。
- `Option`、`Result` 与 `never` 已从 Rust 手工 AST 迁入这份 `.sali` 源，原本由各程序重复声明的
  `Add` 也一并进入 core；编译器启动时严格校验四者的公开形状并登记 lang-item 身份。
- `Add` 进入固定 edition prelude，实现重载时直接写 `extend Number: Add(Number)`，无需在每个
  程序里重新声明 trait。
- 整个依赖图共享同一份 `core` 身份；同名声明、模块或模块 alias 不会获得 `Option`、`Result`、
  `never` 或 `Add` 的语言身份；允许同名的局部类型参数也会正常遮蔽而不触发特殊 lowering。

v0.6.0 补齐数据与公开 API 的模块边界：

- struct 字段和 enum 命名 payload 字段支持默认私有、`pub(package)` 与 `pub`；字段的有效可见性
  不会宽于外层类型，位置 enum payload 继承 enum 的可见性。
- 字段读取、写入、借用、普通与泛型构造、`?.`、类型推断和 `match` 解构使用同一访问检查；
  隐藏字段使类型可以作为不透明值传递，但不能从边界外直接构造或完整解构。
- 模块解析会递归检查函数签名、全局注解、struct/enum 字段和 trait API，禁止较宽 API 泄漏
  较窄名义类型；省略返回类型或全局注解时，语义降低还会检查实际推断出的嵌套类型。
- 固有 `extend` 成员暂时继承目标类型的边界；trait impl 的有效边界由 trait、目标类型及具体
  trait 实参共同收窄，关联类型不能再从这个边界泄漏私有实现类型。
- 检查保留 package identity 与模块祖先关系，因此包根私有类型可安全用于包级 API，而子模块
  私有类型不能被兄弟模块、包级 API 或依赖包洗白。

v0.7.0 扩展 source-backed core 运算协议：

- `Sub(Rhs)`、`Mul(Rhs)`、`Div(Rhs)` 与 `Rem(Rhs)` 和既有 `Add(Rhs)` 一样由普通 core 源声明，
  均带 `Output` 关联类型，并要求对应方法以 `move self`、`move rhs` 接收两个操作数。
- 名义左操作数上的 `+`、`-`、`*`、`/`、`%` 通过编译器匹配的 core trait 唯一候选静态分派；
  期望 `Output` 与整数字面量范围共同参与筛选，两个操作数仍各只求值一次。
- 用户声明的同名 trait 不会伪造 lang-item 身份。整数的五种运算仍是内建 lowering；内建 `/`、
  `%` 遇到除数为零或有符号 `MIN / -1`、`MIN % -1` 时在运行期 trap，对应非法常量在编译期拒绝。

v0.8.0 把 `Copy` 接入 source-backed core 与所有权检查：

- edition core 以普通源码声明 canonical `pub let Copy = trait {}`，编译器严格校验其形状和身份；
  用户包中同名的 trait 不会获得语言语义。
- 整数、`bool`、`()`、`never`、编译器内部错误恢复类型，以及元素为 `Copy` 的 `Array(T, N)`
  由编译器内建为 `Copy`。名义 struct/enum 必须显式写 `extend T: Copy {}`，且所有字段和每个 enum
  variant payload（包括私有表示）都必须递归为 `Copy`。
- 名义 `Copy` 实现只能位于类型定义包。`extend Cell(i32): Copy {}` 只作用于该具体实例，不会泛化
  到 `Cell(bool)` 或模板；当前也不支持 blanket/generic `Copy` impl 或 `where` 证明。
- 未标注参数对 `Copy` 类型默认为复制，否则默认为移动；显式 `move` 始终优先并消费实参。相同判定
  已用于普通读取、闭包捕获以及函数和 bound method 的部分应用。
- 当前函数类型和闭包类型自身仍不是 `Copy`，`Drop` 也尚未公开。

v0.9.0 建立可继续演化的初始化与清理中间层：

- 所有权流状态改为规范化的未初始化 move-path 叶子 alternatives，支持移动 root 或字段后通过
  root 赋值、逐字段赋值恢复初始化，也能在分支 join 与循环回边保留必要的关联信息。
- 精确 alternatives 上限为 64；超过后保守 widened 为“全初始化”与“所有可能未初始化叶子的并集”，
  从而限制分析开销，并且只可能额外拒绝程序，不会错误接受不安全使用。
- `match` guard 不得移动非 `Copy` 的 pattern binding，因为 guard 失败后还可能尝试后续 candidate；
  `Copy` binding 的显式移动仍可用。
- 编译和检查现在都会从真实 HIR 为每个函数建立并验证 `CleanupPlan`：记录作用域、local、move path、
  storage/init/move/overwrite 事件，以及分支、循环、guard、`break` 和 `return` 边。
- 这仍是析构 lowering 的结构基础，不会发出清理代码。未物化资源结果、move-state dataflow、临时值
  liveness、`break` 值传递、借用位置写入、maybe-overwrite、match/pattern 传递、部分应用和闭包捕获
  都以 `PendingCapability` 明确保留。
- 当前没有 `needs_drop`、runtime drop flags、source-backed `Drop`、drop glue 或 LLVM 析构。顶层值仍是
  编译期常量，每次使用独立物化且不进入 cleanup；含资源全局与 `Drop` 的关系会在公开 `Drop` 前定案。

v0.10.0 把资源值的实际落点接入 cleanup CFG：

- `CleanupPlan` 不再只记录“表达式是否有落点”，而是携带具体 destination place；资源型 binding、
  丢弃表达式、赋值、函数尾、显式 `return` 和带值 `break` 都先写入稳定 storage。
- 新的原子 `Transfer` 同时记录 source、destination 以及 initialize/overwrite/maybe-overwrite 状态。
  source 与 destination 必须不同且不能互为投影前缀；每次消费和临时 storage 都由 verifier 双向要求
  对应的 pending dataflow/liveness 标记。
- struct、array、enum、部分应用和闭包按 field、constant index、downcast 与 capture move path 逐子值
  构造；enum 先记录 discriminant，只有全部子值成功后才初始化 root。构造途中 `return`、`break` 或
  发散调用只留下可清理的半成品，不会把 root 或最终返回位置误标为已初始化。
- 调用的值参数、字段/索引 base 以及赋值/返回/`break` 值都经过 staging；旧的“资源结果未物化”和
  “`break` 值尚未传递”两项 pending 已删除。LLVM 析构仍未启用，剩余 move-state dataflow、临时值
  liveness、match/pattern、borrowed mutation 与 capture 细节继续显式 pending。

v0.11.0 完成 cleanup 的静态 move-path forest 与初始化数据流：

- 每个 owned 参数、返回槽、用户/pattern binding 和 planner temporary 都在分析前登记完整 forest；
  struct 的全部字段、enum 的全部 downcast/payload、array 的全部 constant index、`Copy` 与空/ZST
  聚合都保留路径，borrow alias 不建立路径。单函数最多 65,536 个 move path，超出会明确诊断。
- 常量与动态 array index 都按 `Copy` extraction 处理：base 和动态 index 仍各求值一次，但不会消费
  array element，也不会把运行时 index 伪装成有限静态路径。
- cleanup verifier 现在缓存 CFG fixed point：所有路径节点分别维护 `may_init`/`must_init`，在 join
  处取 union/intersection，忽略不可达前驱，并按 operation 位置重放；scope exit、`StorageLive` 和
  `StorageDead` 会清除对应状态。
- enum discriminant、active downcast、字段恢复后的 root 重组、overwrite 失效、Transfer forest 兼容、
  branch condition 和 return place 完整性都进入验证。`MovePathStateDataflow` pending 因此删除。
- Function 类型尚不携带环境布局，具体 callable capture forest 仍由表达式补登记并保持 pending。
  `Init` 仍是幂等的初始化摘要，不表示一次真实写入或旧值析构。

v0.12.0 完成 cleanup 的临时 storage 生命周期数据流：

- 每个 local 现在和 move path 一样在 CFG fixed point 中维护 `may_live` / `must_live`；初始化、移动、
  覆盖、Transfer、discriminant 写入、branch condition 与 return place 都必须位于确定 live 的 storage。
- `StorageLive` 只能从确定 dead 开始；结构化 `StorageDead` 是幂等的作用域结束摘要，可将 live、
  maybe-live 或 dead 统一收束为 dead。它仍不表示已执行析构。
- `while` condition、`while` body 与 `loop` body 使用每轮求值 scope；condition 分支和 body 回边会先
  结束本轮临时值，再进入下一轮，避免循环 fixed point 把同一个 temporary 判成重复开始生命周期。
- `TemporaryStorageLiveness` pending 已删除。下一步是 `needs_drop` 与控制流敏感的 runtime drop
  flags；完成后才从 core 开放 `Drop` 并生成 drop glue。

v0.13.0 重做泛型调用推断语法：

- 完全移除 `_` 类型推断占位符；调用通过省略编译期参数组，从运行时实参和期望结果推断。
- 普通圆括号同时承载显式编译期组和运行时组：类型值组解释为编译期参数，其他组解释为运行时参数。
- 编译期与运行时调用都支持命名实参；`Result(E: bool).Ok(22)` 可只指定部分编译期参数，
  `make(value: 10)` 可明确选择运行时参数。
- `_` 继续用于模式通配符和匿名函数类型槽，但在类型和表达式位置会直接报错。

v0.14.0 完成析构需求与 drop-flag 规划：

- HIR cleanup planner 为每个静态 move path 记录 `needs_drop`；内建 `Copy` 值不需要析构，非
  `Copy` 名义聚合与 callable 先按保守规则保留析构义务。
- cleanup fixed point 在每个 `StorageDead` 前生成树形 drop obligations：确定完整的值静态析构，
  条件完整的值使用稳定编号的 flag，部分聚合只递归到仍可能初始化的字段，避免父子重复析构。
- drop flags 带有随 `StorageLive`、初始化、移动、Transfer、discriminant 更新和 `StorageDead`
  变化的 set/clear action；缓存分析会由 verifier 重算并校验。
- 这一版只建立可执行 lowering 所需的准确计划；尚未公开 `Drop`，也未在 LLVM 中分配 flag 或调用
  析构函数。下一步把 source-backed `Drop`、递归 drop glue 与这些 obligations 一起降到 LLVM。

v0.15.0 开放 source-backed `Drop` 并生成递归 glue：

- edition core 的普通 `.sali` 源声明 `Drop.drop(mut borrow self)(): ()`，编译器按 canonical lang-item
  身份和精确形状登记；同名用户 trait 不会获得析构语义。
- `Drop` 只能由名义类型所在包实现，不能与 `Copy` 同时实现；源码不能直接调用 `Drop.drop`，避免
  自动清理前手工析构造成 double-drop。
- `needs_drop` 现在精确递归：自定义 `Drop` 类型需要 glue，包含它的 struct/enum 只递归清理实际
  需要的字段；enum glue 按运行期 discriminant 选择 active variant。
- LLVM 已生成并验证 custom-drop、struct、enum 的递归 glue，且原生链接测试通过；scope-exit 调用、
  flag storage/branch 和 unwind 仍在下一版接入，因此当前还不能依赖析构副作用发生。

标准库已经从 v0.5 的 `core` 引导开始，并按 `core → alloc → std` 分层推进。v0.6 封闭了库 API
所需的字段与签名边界，v0.7 将首组五个算术协议完整迁入 source-backed core，v0.8 完成第一阶段
`Copy`，v0.9 建立 cleanup CFG，v0.10 补齐资源 storage/transfer，v0.11 完成完整 move-path forest 与
初始化 fixed point，v0.12 再完成 temporary storage liveness，v0.14 已加入 `needs_drop` 与控制流敏感
drop-flag 计划；这些都属于已经开始的 `core` 阶段，但还不发出析构。下一步提供 source-backed
`Drop` 与递归 drop glue；下一步把 v0.14 的 obligations/flags 降为真实 scope-exit 调用，其后才固定
raw pointer 与 allocator ABI 并进入 `alloc`。平台 `std` 的 IO、文件、环境与进程放在 C ABI 和最小
运行时之后。

最小示例：

```sali
let add(x: i32)(y: i32): i32 = x + y

let main(): i32 = add(1)(41)
```

捕获闭包目前采用保守子集：多参数组必须在一次调用链中完整应用，且只支持受限捕获形状和直接调用。
当前借用期采用词法范围。固定数组只允许 `Copy` 元素并仅支持只读索引；循环回边禁止移动
循环外部绑定，以保证下一轮仍拥有相同的可用值。方法的临时 receiver 目前需先绑定到局部；bound
method 的部分应用只允许捕获 `Copy` receiver 和实参。名义 `Copy` 必须以具体、同包且结构合法的
实现显式选择加入；尚无 blanket/generic impl 或 `where` 证明，函数类型和闭包类型也不实现 `Copy`。
表达式路径中的 `Self` 与
`A.method(a)()` 完全限定调用尚未开放。省略编译期组的嵌套推断仍受当前表达式类型探测能力限制；
无法唯一推断时需用 `T: Concrete` 形式的命名编译期实参。trait 首版
暂不支持 `where`、泛型 impl、默认方法、泛型关联类型、完全限定调用和 trait object；无约束的抽象类型
不能声明为 `copy` 参数。`Add`、`Sub`、`Mul`、`Div` 与 `Rem` 已由 edition core 登记为 lang item，
但重载路径目前仍要求左操作数能静态探测为具体名义类型；多个 `Rhs` 候选并存时，无法静态探测的
复杂右操作数需要先绑定到带类型标注的局部量。`Option` / `Result` 构造器既可使用
`Option(i32).Some(1)` 这样的显式类型头，也可在证据充分时写 `Option.Some(1)`；
`??` 首版直接识别这两个预导入容器，尚未开放用户 `Coalesce` 实现。`.try` 首版只在显式标注
`Option` / `Result` 返回类型的具名函数中建立传播边界；`Result` 错误类型必须完全相同，尚未开放
`Try` / residual 转换、传播块或闭包传播。`throw` 复用同一具名 `Result` 边界且暂不执行错误类型
转换。`?.` 首版直接识别拥有的预导入容器，尚未开放用户 `Chain` 实现、借用容器、可变借用
receiver、方法部分应用或可调用字段。五个算术协议之外的其他运算符 trait 和异步也尚未开放。

文件模块当前尚未实现 glob import；固有 `extend` 成员还没有独立的显式可见性语法，当前继承
目标类型的边界。
解析期 API 诊断带 source path，降低后才发现的推断 API 或字段访问诊断还没有逐 item source map。
依赖目前仅支持本地路径，没有 registry、Git、多版本求解或校验和。
