# Salicin 语言规范

状态：演进中的设计规范  
目标：静态类型、静态编译、LLVM 后端、默认内存安全、支持所有权与柯里化。  
本文中的语言名称为 **Salicin**。

本文定义“源程序是什么意思”，不把 LLVM 的实现限制暴露成语言规则。实现状态与未完成工作记录在
[项目状态](../project/status.md)，版本变化记录在[更新日志](../../CHANGELOG.md)。Salicin 源文件统一
使用 `.sc` 后缀；它可理解为 successor C 或 super C 的简写，但这两个展开不构成语言正式名称。

## 1. 设计原则

1. `let` 是统一的不可变名称绑定语法，可以绑定值、函数、类型、trait 和模块。
2. `let mut` 只建立可重新赋值的值绑定，不允许用来改变类型、函数、trait 或模块。
3. 所有表达式都有类型；无结果表达式的类型只写作 `()`。
4. 函数默认柯里化。每一对参数括号形成一个参数组。
5. 值默认不可变；资源类型默认移动，可复制类型默认复制。
6. 泛型在静态编译时单态化，运行时不保留 `type` 参数。
7. trait 同时承担约束、静态分派和运算符协议的职责。
8. 错误传播、可空传播和异步挂起是不同效果，使用不同协议。

## 2. 词法约定

- 标识符区分大小写并按 Unicode NFC 规范化。普通源码标识符采用 Unicode XID 规则；编译器应对
  容易混淆的跨文字系统字符发出警告。包名、文件模块名和 FFI 链接名另行限制为 ASCII。
- 建议类型和 trait 使用 `UpperCamelCase`，值、函数、显式模块和文件模块使用
  `lower_snake_case`；命名风格不是语义规则。
- Salicin 源文件使用 `.sc` 后缀；UTF-8 是唯一源码编码。
- `//` 到行尾是行注释，`/* ... */` 是可嵌套块注释。
- lexer 保留逻辑换行。未闭合圆括号或方括号内的换行被忽略；上一 token 是运算符、逗号、`.`、
  `?.`、`=`、`=>`、`->` 或 `:` 时，下一物理行继续当前表达式。其他表达式后的换行形成语句
  边界。普通调用不能从下一逻辑行的 `(` 继续，因此 `f\n(x)` 是两个表达式而不是 `f(x)`。
- 换行只分隔语句，不自动丢弃块尾值；显式 `;` 才强制丢弃表达式值。同一逻辑行包含多个语句时
  必须用 `;`。
- 关键字至少包括：`let`、`mut`、`copy`、`move`、`borrow`、`type`、
  `struct`、`enum`、`trait`、`extend`、`match`、`return`、`throw`、`if`、
  `else`、`loop`、`while`、`for`、`in`、`break`、`continue`、`do`、`try`、
  `async`、`pub`、`use`、`as`、`where`、`region`、`extern`、`unsafe`、`root`、
  `super`、`package`。
- `self`、`Self`、`root`、`super`、`true`、`false` 是保留字。
- `access`、`passing` 和 `effect` 只在编译期 kind 位置具有上下文含义，不是全局保留字。
- region 名以 `'` 开头，后接普通标识符主体，例如 `'a`、`'input`；`'static` 是预定义 region。

## 3. 声明与作用域

### 3.1 不可变和可变绑定

```sc
let answer = 42
let answer: i32 = 42
let mut count = 0
count = count + 1
```

`let` 建立不可重新赋值的绑定。不可变约束作用于绑定，而不自动承诺其引用对象不存在
内部可变性。`let mut` 允许重新赋值，但新值必须与绑定的静态类型相同。

同一词法作用域不允许重复声明同名绑定；内层作用域可以遮蔽外层绑定：

```sc
let x = 1
do {
  let x = "one" // 合法，遮蔽外层 x
}
```

变量必须在使用前完成初始化。不提供“声明但未初始化”的安全语法。

### 3.2 顶层声明类别

名字位于不同的语义类别。类型、trait、effect、access、模块和全局值使用同一顶层声明冲突规则；
普通具名函数有独立的函数重载集，因此可以和类型同名，用作显式 constructor/factory：

```sc
let n = 1                         // 值
let add(x: i32)(y: i32) = { x + y } // 具名闭包声明
let Point = struct { x: i32, y: i32 } // 类型
let Point(x: i32, y: i32): Point = { Point { x: x, y: y } } // 同名普通函数
let Display = trait { ... }       // trait
let Math = struct { ... }         // 模块
```

顶层值必须能在编译期初始化；需要运行期初始化的全局状态应由显式初始化函数或惰性容器
提供。普通模块级 `let mut` 被禁止；共享可变状态使用 `Atomic`、`Mutex` 等安全容器，或声明
`unsafe let mut` 并在每次访问时承担同步与别名责任。编译期顶层值不依赖声明顺序，但其常量求值
依赖图必须无环。

`let` 右侧的 kind 决定绑定类别：`let Index = usize` 建立类型别名，`let n = 1` 建立值。
同一名字不能重复绑定两个类型/模块/全局值类别；类型名和具名函数名可以相同。此时 `Point { ... }`
总是结构体字面量，`Point(...)` 总是普通函数调用。具名函数还可以按第 18 节形成重载集。具名函数
在自身函数体中可见以支持递归；普通值绑定在 initializer 完成前不可见。

### 3.3 可见性

声明默认对所在模块及其子模块可见。`pub(package)` 对当前包公开，`pub` 对所有依赖者公开：

```sc
pub let f(x: i32) = { x }
pub let Point = struct { pub x: i32, pub y: i32 }
```

公开声明不能在签名或公开字段中泄漏可见性更低的类型。导入不会提升可见性；`pub use` 是显式
重导出。完整模块规则见第 11 节。

数据字段使用同一套三级可见性，且有效范围是外层类型与字段声明范围的交集。例如公开 struct 的
私有字段仍只对声明模块及其子模块可见，`pub(package)` struct 中写 `pub` 字段也不会越过包边界。
命名 enum payload 可以逐字段标注；没有字段标注位置的 positional payload 继承 enum 可见性。
字段访问、写入、借用、构造、可选链和模式解构都检查这个有效范围。当前构造器与 payload pattern
必须列全字段，因此只要存在调用者不可见的必填字段，调用者就不能直接构造或完整解构该值，但仍可
通过公开 factory 持有、移动和传递这个不透明类型。

签名泄漏检查递归进入数组与泛型实参。显式参数/返回类型、全局注解、字段、enum payload、trait
方法和关联类型默认值在模块解析后检查；省略返回类型或全局注解时，再以语义推断出的实际类型检查。
类型参数、`Self` 和 trait 关联类型占位符不被误当成较窄的名义声明。

在独立成员 visibility 语法落地前，固有 `extend` 方法、关联函数和关联常量继承目标类型的边界。
trait impl 的可调用边界是 trait、目标具体类型和 trait 实参可见范围的交集；实现提供的关联类型
必须至少覆盖这个交集，不能让公开方法调用产生调用者无权命名的私有结果。

## 4. 基础类型与类型表达式

首批内建类型：

```text
i8 i16 i32 i64 i128 isize
u8 u16 u32 u64 u128 usize
f32 f64
bool char () type
```

- `()` 是单元类型，只有一个值 `()`。
- `type` 是类型参数的 kind，只能出现在编译期参数位置，不能作为普通运行时值类型。

`Never` 不是额外的原始类型；edition prelude 包含以下普通声明：

```sc
let Never = enum {}
```

无结果类型统一写作 `()`，不提供 `void` 别名。`Never` 是零 variant 枚举，因此没有任何值。
`return`、`throw`、无可达 `break` 的循环和其他不终止表达式
具有 `Never` 类型，并可强制转换到任意期望类型。用户声明的其他零 variant 枚举同样是
uninhabited type；对其做空 `match {}` 会产生 `Never`。

当前引导实现从普通 `core` 源解析 `Never`。

整数文字先作为“未定整数”参与推断；若上下文没有约束，默认 `i32`。有符号整数溢出在
debug 构建中检查，release 构建默认二进制补码回绕；可另行提供显式 checked/wrapping API。
内建整数 `/` 与 `%` 在除数为零时 trap；对有符号整数，`MIN / -1` 与 `MIN % -1` 也 trap，避免
进入 LLVM 的未定义算术。编译期常量求值会直接拒绝这些情况，而不是生成只能在运行期失败的值。

类型应用使用普通调用外形；结构体值构造使用第 8 节的 braced literal：

```sc
Option(i32)
Result(IoError)(i32)
Future(i32)
A
```

`Option` 和 `Result` 是 `core` 根模块中的普通标准库定义；源码命名它们时需要 `use core.Option`
或 `use core.Result`。它们不是 prelude 名称。`Result` 按错误类型优先柯里化为
`Result(Error)(Value)`，因此 `Result(Error)` 本身就是一元类型构造子。

类型构造子的编译期参数名也是调用标签。类型位置可以用具名实参消歧或提高可读性：

```sc
let Pair(K: type, V: type) = struct { key: K, value: V }
let pair: Pair(V: bool, K: i32) = Pair(K: i32, V: bool) { key: 1, value: true }
```

标签按构造子的声明参数匹配并在类型检查前归一化到声明顺序；归一化后
`Pair(V: bool, K: i32)` 与 `Pair(i32, bool)` 是同一类型。标签不参与类型等价，也不形成新的
重载维度。一个类型实参组要么全部具名，要么全部按位置书写。

`_` 不参与任何类型或编译期参数推断，也不是类型或表达式；类型参数、值参数、region 参数及其
命名实参均不能用 `_` 占位。泛型调用通过省略编译期参数组触发推断；`_` 只保留在模式通配符和
匿名函数类型槽等本来就表示“忽略名称”的位置。

### 4.1 复合类型与字面量

首批核心复合类型为：

```sc
(i32, String)       // 元组
Array(i32, 4)       // 固定长度数组；长度是编译期 usize
Slice(i32)          // 连续元素的非拥有视图
Str                 // 不可变 UTF-8 字符串视图
String              // 拥有的 UTF-8 字符串
```

对应字面量：

```sc
let pair = (1, "one")
let singleton = (1,)
let values = [1, 2, 3, 4]
let text: Str = "salicin"
```

`()` 是零元元组；一元元组必须保留尾逗号。数组的所有元素具有同一类型，数组长度参与类型。
`Slice(T)` 和 `Str` 不拥有其数据，普通使用必须通过 `borrow`；`String` 和动态容器拥有数据。
字符串以 UTF-8 存储，`char` 表示一个 Unicode scalar value，而不是一个字节。按整数索引字符串
不属于核心操作；标准库提供按字节、scalar 或 grapheme 遍历的显式 API。

整数和浮点数字面量可带类型后缀，例如 `42u64`、`3.5f32`。字符使用单引号，字符串使用双引号；
转义至少包括 `\\n`、`\\r`、`\\t`、`\\0`、`\\xNN` 和 `\\u{...}`。源码中不存在 `null`；
可能缺失的值使用 `Option(T)`。

## 5. 函数、参数组与调用

### 5.1 函数声明

```sc
let f(x: i32) = {}
let fc(x: i32)(y: i32): i32 = { x + y }
let unit(): () = { () }
```

每一对括号形成一个参数组。`fc` 的抽象类型写作：

```sc
(i32): (i32): i32
```

函数签名允许保留参数名：

```sc
(x: i32): (y: i32): i32
```

参数名用于文档、诊断，以及在函数声明中把名字引入函数体，但不参与类型相等性。因此
`(x: i32): i32` 与 `(value: i32): i32` 是同一函数类型。`->` 不用于函数类型，它只在闭包
字面量中分隔显式参数和闭包体。

```sc
let f(x: i32) = {}
```

是把参数提升到绑定名称旁边的闭包声明糖，等价于：

```sc
let f: (i32): () = { (x: i32) -> () }
```

具名形式中的参数组定义闭包参数，右侧必须是花括号闭包体；`let f(x) = expression` 不属于语法。
实现可以为递归、泛型单态化和稳定符号保留专用 AST/ABI，但语言语义仍是闭包绑定。普通函数值
绑定使用无名签名和显式参数闭包：

```sc
let succ: (i32): i32 = { (n: i32) -> n + 1 }
```

所以两种声明在类型和行为上等价。具名闭包在自身闭包体中可见，从而支持递归。

省略返回类型时从函数体推断。公开 API 建议强制写返回类型，递归函数必须写返回类型。

### 5.2 柯里化

调用一次只消费一个参数组：

```sc
let add(x: i32)(y: i32) = { x + y }
let add_one = add(1) // 类型为 (i32): i32
let three = add_one(2)
let also_three = add(1)(2)
```

`f(a, b)` 是一个包含两个参数的参数组；`f(a)(b)` 是两个各含一个参数的组，二者类型不同。
部分应用产生闭包，其环境保存已经传入的参数。保存方式服从参数的传递模式：`copy` 复制、
`move` 转移、`borrow` 借用。

零参数组 `()` 不是多余语法。它表示显式延迟调用：

```sc
let make_logger(config: Config)(): Logger = { ... }
let logger = make_logger(config)()
```

### 5.3 命名实参

函数、方法、闭包和构造器都允许按位置或按名称传参；同一参数组不能混用两种形式：

```sc
make(value: 10)
subtract(left: 44, right: 2)
```

运行时命名实参使用声明中的参数名，并按参数声明顺序书写，因此仍保持实参从左到右求值。参数名
一旦用于外部调用就属于源代码 API。编译期命名实参还可只给出一组中的部分参数，以消除省略组和
运行时组之间的歧义；未给出的参数继续由上下文推断。

### 5.4 尾随闭包

```sc
let value = f(x) { (n: i32) -> n + 1 }
```

严格脱糖为：

```sc
let value = f(x)({ (n: i32) -> n + 1 })
```

尾随闭包总是新建一个只含该闭包的参数组，不会加入前一组。所以下面两种调用不等价：

```sc
f(x) { (n: i32) -> n + 1 }    // f(x)({ ... })
f(x, { (n: i32) -> n + 1 })   // f(x, { ... })
```

接收尾随闭包的函数应把闭包放在独立的最后参数组中：

```sc
let map(T: type)(U: type)
  (items: List(T))
  (transform: (T): U): List(U) = { ... }

let names = map(T: User)(U: String)(users) { (user: User) -> user.name }
```

一条调用表达式只允许一个尾随闭包。需要传递多个闭包时，其余闭包使用普通参数组显式传入。
尾随闭包必须在同一逻辑行紧跟一个已经含显式参数组的调用，所以允许 `f(x) { ... }`，不允许
`f { ... }`。尾随闭包之后仍可继续成员访问或普通调用。

### 5.5 函数类型与应用时机

函数类型中的冒号右结合：

```sc
(i32): (i64): bool
```

解析为 `(i32): ((i64): bool)`。参数组数量和每组 arity 都是类型的一部分，因此
`(i32, i64): bool` 与 `(i32): (i64): bool` 不是同一类型。参数传递模式也属于函数类型，
参数名则不属于：

```sc
(copy _: i32): bool
(move _: i32): bool       // 与上一类型不同
(value: i32): bool        // value 不参与类型相等性
```

匿名槽中的显式模式必须写 `_:`，以区分“借用一个 `T`”和“按默认模式传递一个已有借用值”：

```sc
(borrow _: T): U       // 参数模式是 borrow，参数底层类型是 T
(_: borrow T): U       // 参数模式是 auto，参数值类型本身是 borrow T
```

命名函数和闭包都是一等值，可以保存、作为参数传递或返回。每次应用参数组都会调用当前函数层，
并立即从左到右求值该组实参。多参数组声明是嵌套函数层的简写，其源码函数体属于最内层；外层
只完成参数绑定并返回下一层，因此在最后一组应用前不会执行该源码函数体：

```sc
let f(x: Resource)(y: i32) = { use(x, y) }
let pending = f(resource) // resource 此处已按参数模式移动或借用；函数体尚未执行
let result = pending(1)   // 此处进入函数体
```

但显式返回闭包的单组函数可以在第一组调用时执行代码：

```sc
let make_adder(x: i32): (i32): i32 = {
  log("creating adder")
  { (y: i32) -> x + y }
}
```

部分应用结果是编译器生成的闭包。连续应用可以在不改变上述可观察行为的前提下优化为直接调用。
函数值比较没有内建语义，不自动实现 `Eq` 或 `Hash`。

### 5.6 Effect 与 handler

effect row 是函数签名的编译期元数据，以返回类型后的 `with(...)` 子句书写。没有该子句就是
pure；effect 的顺序不影响语义，同一项不能重复。`with` 是上下文词。该子句不接收运行时实参，
也不增加一层柯里化：

```sc
use core.effects.{Throws, Unsafe}

let Error = enum { NullPointer }

let read(pointer: Ptr(i32)): i32 with(Unsafe) = { *pointer }
let fallible(): i32 with(Throws(Error)) = { throw(Error.NullPointer) }
let combined(pointer: Ptr(i32)): i32 with(Throws(Error), Unsafe) = {
  if pointer.is_null() { throw(Error.NullPointer) }
  *pointer
}

let forward(pointer: Ptr(i32)): i32 with(Unsafe) = { read(pointer) }

let main(): i32 = {
  let value = 42
  unsafe { forward(Ptr(borrow value)) }
}
```

`: T with(Unsafe)` 允许函数体执行原始指针等不安全操作，并要求完整调用发生在另一个含 `unsafe` 的函数或
`unsafe { ... }` handler 内。柯里化函数只在应用最后一个参数组、真正进入函数体时产生 effect；
部分应用本身不会产生该 effect。effect 行参与函数类型相等性，因此 trait requirement 与
implementation 必须具有相同的 effect。

`: T with(Throws(E))` 表示函数的逻辑成功类型为 `T`，执行时可能以 `E` 中止当前 effect 边界。
调用另一个具有相同 `Throws(E)` 的函数会自动传播；源码不写逐调用的 `.try`。
`core.effects.Throws(Error)` 声明普通 abort operation `raise(move error: Error): Never`。这个
operation 使用代数 effect 的普通 handler 规则；它的 clause 不带 `resume`，直接产生 handler
答案。如果当前 row 中有且只有一个 `Throws(E)`，`throw(error)` 会按
`Throws(E).raise(error)` 处理；存在多个 `Throws` row 时必须显式调用对应的
`Throws(E).raise`。

`try { body }` 是 handler：它移除 `body` 的 `Throws(E)` 要求并产生 `Result(E)(T)`。handler
内的正常尾值成为 `Ok`，传播出的错误成为 `Err`。当 `try { body }` 有显式 `Result(E)(T)` 上下文，
编译器会生成普通 `Throws(E).handle`：正常完成经 `done` 变为 `Ok`，`raise` 变为 `Err`。无上下文时，
直接调用普通 `Throws(E)` 函数或局部函数值，可在成功类型可探测且错误类型唯一时推断 `Result(E)(T)`；
无法唯一确定时，用 `use core.Result` 后的 `let result: Result(E)(T) = try { ... }` 提供上下文，
或先把错误显式转换为同一种类型。普通 `Option` 和 `Result` 仍是普通数据类型；`?.`、`??` 与显式
`match` 用来操作它们，不会凭返回类型隐式获得 throws 语义。

`unsafe { ... }` 处理并移除 `Unsafe`，`do { ... }` 不处理 effect，只原样转发。调用要求可以用
`E: effect` 参数化：

```sc
let tagged(E: effect)(value: i32): i32 with(E) = { value }
let forward(E: effect)(value: i32): i32 with(E) = { tagged(E)(value) }
let UI = effect
let render(): i32 with(UI) = { 42 }
let invoke(E: effect)(action: (): i32 with(E))(): i32 with(E) = { action() }
let screen(): i32 with(UI) = { invoke(render)() }

let ordinary = forward(40)                 // E 默认 pure
let colored = unsafe { forward(Unsafe)(2) }
let named = unsafe { forward(E: Unsafe)(2) }
```

`let UI = effect` 声明无 operation 的名义 marker；其身份遵循普通模块路径和可见性，公开 API
不得泄露私有 effect。effect 也可以接受类型参数并声明 operation requirements：

```sc
let State(S: type) = effect {
  let get(): S
  let put(move value: S): ()
}

let read(): i32 with(State(i32)) = { State(i32).get() }
```

operation 没有函数体，完整调用产生所属的已实例化 effect。`State(i32)`、`State(i64)`以及其他
模块中同名的 `State(i32)`都是不同的 row 成员。operation 的参数组、传递模式、返回类型和附加
`with(...)` 要求按普通函数签名检查；部分应用本身仍是 pure。同名 operation 只能按运行时参数名
重载，不能按类型重载；重载调用必须使用按声明顺序排列的具名参数。handler 中可以重复同一个
operation label，其 clause 在 `resume` 之前重复对应的参数名以完成消歧。当前实现阶段已经提供声明、名义
identity、传播和类型检查。派生的 `State(i32).handle(get: { (resume) -> ... }, ...) { action }`
可处理 action 中词法可见的 operation；`resume` 是一次性 continuation，也可以不调用以中止剩余
计算。普通具名完整调用会在 handler 下特化成真实的局部 closure frame；参数保留原本的 copy、move
或 borrow mode，显式 `return` 以该 frame 为边界，callee 局部值也会在调用者 continuation 恢复前
清理。frame 通过带显式 tail terminator 的一次性 CPS continuation 完成，因此 clause 不调用
`resume` 会中止完整的跨函数剩余计算，调用 `resume` 后也可以继续组成 handler 答案。直接递归和
effectful `while`、`loop` backedge 使用 CPS lifted frame。具体 continuation closure 会擦除成包含
call entry、drop entry、environment pointer 与 one-shot flag 的统一隐藏值；named frame 显式接收该
隐藏参数，每个直接或互递归调用点为自己的剩余计算创建新 node，因此递归函数结果类型可以不同于
完整 handler answer。调用 node 会把 environment 移交给 call entry 并解除其 armed 状态；放弃
armed node 则调用 drop entry，所以两条终止路径都会让 move 捕获值恰好析构一次。
定义 handler 的函数可以把带同一 effect 的 callable 作为参数。具名函数、无捕获别名和有限具名
分支会按目标特化。捕获 action 切片还允许：带显式函数类型的闭包 binding 稍后直接传入完整 handler
调用，调用可位于块尾、普通初始化式、表达式语句或更大表达式内部。闭包环境仍在声明点建立，保留原始
borrow/move 时机；调用点从该环境提升字段，其中共享 `Copy` 为 `borrow`、可变 `Copy` 为
`borrow(mut)`、消费 owned root 为 `move`。闭包注入词法 action 后执行 selective CPS，恢复与放弃
路径都保证 move 资源只析构一次；消费 action 后捕获借用被释放。局部 callable alias 的 move 会继续
携带原 action 身份，且借用捕获按指针槽重定位、owned 捕获按值与 drop flag 重定位。
完整调用也可以直接写尾闭包 literal；编译器把它物化为带 handler 参数函数类型的内部局部 binding，
再进入同一捕获提升路径。action 之前的 `copy`、`move` 参数（包括更早的柯里化参数组）会先按源码
顺序物化为带参数类型的内部局部值，确保副作用和所有权转移发生在闭包捕获之前。此前若有 `borrow` 或
`borrow(mut)` 参数则暂不改写，以免把 place 借用错误地延长或转换成 owned 临时值；条件 action 值、
跨函数传递和任意擦除 action 仍待后续 ABI。
数组元素、索引、普通与可空成员、`match` scrutinee/arm body 以及 `do`、`unsafe`、`try` 中的
operation 按源顺序进入 selective CPS，`&&` 与 `||` 保持短路。`??` 的 scrutinee 与 fallback 都可
挂起，且 fallback 仍只在 `None` 或 `Err` 路径求值。完整可空方法调用会先求值 owned receiver，仅在
`Some` 或 `Ok` 路径进入参数 CPS，并把调用结果重新包装后继续；`None` 或 `Err` 会跳过全部参数。
match guard 可包含直接 operation 或传播同一 effect 的具名调用；guard 为假会继续
尝试后续 arm。当前该路径要求完整 match 输入实现 `Copy`，非 Copy scrutinee 的 continuation ownership
仍待专用 lowering。捕获型间接调用和最终通用 continuation ABI 按
[代数效应设计](algebraic-effects.md)继续实现；尚未覆盖的路径会被拒绝，不能让带 operation 的
effect 逃逸原生入口。

不同 identity 的词法嵌套 handler 按源代码顺序组合。外层 handler 会穿过内层 handler 的 action、
operation clause、`done` clause 以及编译器生成的 named-call frame/continuation closure，处理其中属于
自己的 operation；内层 clause 的 `resume` 参数遮蔽外层同名 continuation。相同 identity 的嵌套仍由
最近边界处理，不会合并。

`effect` kind 的实参是完整 row，包括 `pure`、`Unsafe`、`Throws(E)`、异步挂起、名义 marker 及其
组合。row 本身不作为运行时值，但其中的控制 effect 可以影响 lowering 和 ABI；`pure` 是省略且
没有其他约束时的默认值。effect 参数只能声明在函数或泛型 inherent member 上，只能用于
`with(...)` 或转发给另一个 effect 编译期参数，不能作为运行时类型。
`: T with(Unsafe, E)` 表示固定要求
`Unsafe` 再与 E 合并。选择 `Unsafe` 的实例可在函数体执行 unsafe 操作；选择或默认 `pure` 的实例
仍会被相同的静态检查拒绝。不会引入方括号或 `_` 推断语法；旧的 `T ! effect` 拼法在 1.0 前直接
删除。

effect 参数抽象整个 row，包括 `Throws(E)` 与未来的异步挂起。实例化后再决定控制流变换和 ABI，
不能通过是否改变 carrier 把它们拆成第二套泛型机制。当前引导编译器正在把旧内部错误 carrier
迁移到普通 `Throws(E)` effect；未来加入其他控制 effect 时，同样必须完整保留 row，不能静默丢掉其中的项。

这些内建能力也必须有 edition 固定的源码契约。Salicin 的设计目标是先把标准控制能力表达为普通
effect、trait 或协议，再由编译器校验对应 lang item 并做 lowering；语法糖应尽量脱糖到这些
源码级契约，而不是为每个类型增加封闭的编译器特例。`core.effects` 声明普通 effect 形态的 `Unsafe`、
带普通 abort operation `raise(move error: Error): Never` 的 `Throws(Error)`，以及带最小
`suspend(): ()` operation 的 `Async`；`core.access` 声明 `Shared` 与 `Mutable`；`core.control`
声明 `do`、`try`、`throw`、`unsafe`、`loop` 的控制函数签名；`core.functional` 声明使用构造子 kind 的 `Functor`、
`Applicative` 与 `Monad` 协议。编译器只对通过 core bundle 形状校验的声明赋予 lang-item 身份。
effect 是类型级名义成员，声明名以及 `with(...)` 中路径的最后一段必须大写开头，例如 `UI`、
`State(S)`、`core.effects.Throws(E)`；小写的 `with(foo)` 不会作为兼容拼法或隐式自定义 effect
保留。effect row 参数（例如 `E: effect`）按参数名解析，不受该名义命名规则限制。
控制函数的函数体由编译器提供，因为它们会建立、消除或转发非局部控制边界。普通包不能声明无函数体的
顶层函数，也不能用同名声明获得特殊语义。`core.control` 还声明空结构契约 `Continuation(Input, Output)` 与
`EffectCallable(Input, Output, Answer)`；前者表示一次性续体，后者表示携带 call/drop 入口、环境与
所有权标志的擦除 handler action。其内部调用入口接收 `Input` 与
`Continuation(Output, Answer)` 并返回 `Answer`；擦除和调用都转移所有权。字段与低层操作属于
编译器 ABI，不暴露为普通结构体数据或标准库函数。
未来实现 async 时，必须在同一个实现切片加入 `Future`、`async` 与 handler 契约；当前 `Async`
只有普通的 `suspend(): ()` operation，不代表 `await` 已经可执行。

函数类型使用同一位置表达 effect，例如 `(i32): i32 with(Unsafe)`。因此不同 row 的 callable 是
不同类型，高阶函数可以用 `with(E)` 约束并从实参 callable 的签名推断 E。row 按“调用要求”取
子类型：实际 callable 的要求集合是期望集合的子集时可以赋值。因此 pure callable 可用于
`with(UI)` 或 `with(Unsafe)` 的槽位，反向转换不成立；拓宽后的间接调用仍按槽位声明的 row 检查。
`E` 从 callable 实参推断其精确实际 row，不因外层期望类型而静默拓宽。trait requirement 与
implementation 仍要求签名完全一致，而不是借子类型关系改变协议契约。

`throw` 和具有 `Throws(E)` 的完整函数调用都产生参数化错误 effect；最近的 `try { ... }` 或同错误
类型的 `Throws(E)` 函数边界负责处理或传播。类似地，具有异步 effect 的调用直接产生挂起点，
`async { ... }` 处理该 effect 并构造 Future，不再使用逐调用的 `.await`。`Unsafe`、`Throws(E)` 与
异步挂起共享 effect row 和高阶函数转发机制，但 handler 的 lowering 各不相同。

首批内建 effect 限定为 `Unsafe`、`Throws(E)` 和异步挂起。生成器加入时可再增加参数化的
`yield(Y)`。IO、分配、普通可变状态和不可恢复终止暂不作为语言内建 effect：它们分别由能力类型、
所有权/借用、库 API 与显式进程语义表达，避免让所有普通函数签名携带过度细碎的 effect。

## 6. 参数传递与所有权

```sc
let f(
  copy a: i32,
  move b: Buffer,
  borrow c: Document,
  borrow(mut) d: Canvas,
) = {}
```

传递模式定义如下：

| 模式 | 调用效果 | 函数体能力 |
|---|---|---|
| `copy T` | 复制实参；要求 `T: Copy` | 拥有独立值 |
| `move T` | 转移所有权；原绑定之后不可用 | 拥有值 |
| `borrow T` | 建立共享借用 | 只读访问 |
| `borrow(mut) T` | 建立排他借用 | 可变访问 |
| 未标注 | `T: Copy` 时为 `copy`，否则为 `move` | 同对应模式 |

`borrow(mut)` 是 `borrow(A)` 在 `A = mut` 时的直接写法；`mut` 是 access 实参，而不是重新绑定
参数。旧式前缀拼写不属于语言语法。

核心借用规则：

1. 任意时刻可以存在多个共享借用，或一个排他借用，但不能同时存在。
2. 借用不能长于其来源。
3. 移动后绑定不可再使用；给该可变绑定重新赋值后可再次使用。
4. 部分移动后只允许访问尚未移动的字段。
5. `Copy` 类型的普通读取不发生移动。
6. 返回值和闭包捕获都参与生命周期检查。

借用检查采用基于最后一次使用的非词法生命周期。局部生命周期尽量推断；跨结构体保存借用或
公开签名无法唯一推断来源时，使用 6.4 节的显式 region 参数。

### 6.1 显式模式优先于类型默认值

显式 `copy`、`move`、`borrow` 和 `borrow(mut)` 永远优先于默认规则。特别地，即使 `i32`
实现了 `Copy`，传给 `move value: i32` 也会在语言语义上使调用方的原绑定失效。优化器可以消除
机器层面的复制，但不能让已移动绑定重新可用。这使泛型 API 能明确表达“消费一次”的协议。

`borrow(mut)` 的实参必须是可变且可寻址的 place expression，例如可变局部、可变字段或可变解引用；
临时计算值不能作为可变借用实参。共享借用可以短暂借用临时值，但该借用不能逃出当前完整表达式。
部分应用保存的借用从应用该参数组时开始，并持续到部分应用闭包最后一次使用或析构。

对未标注的泛型参数，传递模式保留为 `auto`，并在单态化时依据实际类型是否实现 `Copy` 决定。
如果 API 需要对所有实例保持相同的消费行为，必须显式写 `copy` 或 `move`。

### 6.2 Access 关键字泛型

共享和排他访问是编译期能力值，可由 `access` kind 参数化：

```sc
let identity(A: access, 'a: region, T: type)
  (borrow(A, 'a) value: T): borrow(A, 'a) T = {
  borrow(A, 'a) value
}

let shared = identity(T: i32)(value)        // A 默认推断为 shared
let exclusive = identity(A: mut, T: i32)(mutable_value)
let also_exclusive = identity(mut, i32)(mutable_value)
```

`access` 只有两个内建值：`shared` 和 `mut`。它们不是运行时值，不可保存进普通变量；每个不同的
access 实参参与单态化。`borrow(A)` 可出现在参数传递模式、借用类型与借用表达式中。实例化为
`shared` 时等价于 `borrow`，实例化为 `mut` 时等价于 `borrow(mut)`。省略且没有其他约束时采用
`shared`，需要消歧时使用普通命名编译期实参 `A: mut`，不引入方括号语法或 `_` 占位。

access 参数统一的是同一算法的访问能力，不是函数 effect：它不会表示抛错、异步、IO 或状态修改。
`type`、`region`、`access`、`passing` 和 `effect` 都描述编译期数据或调用约定；effect 行使用独立
kind，不能误用 access 参数表达控制要求。

### 6.3 Passing 关键字泛型

按值传递策略可由 `passing` kind 参数化，并直接在原本写 `copy` 或 `move` 的关键字位置引用：

```sc
let identity(P: passing, T: type)(P value: T): T = { value }
let forward(P: passing, T: type)(P value: T): T = { identity(P, T)(value) }

let copied = identity(copy, i32)(number)
let consumed = identity(P: move, T: Buffer)(buffer)
let automatic = identity(resource) // P 默认 auto，T 由 value 推断
```

`passing` 的内建值是 `auto`、`copy` 和 `move`。`auto` 对实现 `Copy` 的实际类型选择复制，否则
选择移动；`copy` 要求实际类型实现 `Copy`；`move` 即使用于 Copy 类型也会在语言语义上消费原绑定。
三种实例在函数体内都提供拥有值，因此同一个泛型函数体可以安全地返回、保存或继续转移参数。
passing 值不进入运行时，但参与单态化；可使用位置或命名编译期实参，省略时不需要 `_`。

借用没有塞进 `passing`：借用还携带共享/排他能力、来源 region 和不同 ABI，由正交的
`borrow(A)` 表达。这样 `passing` 只改变调用方的按值所有权效果，`access` 只改变借用能力。
`P: passing` 当前只允许声明在函数或扩展成员上，不能作为数据类型、trait 或 extend header 参数。

### 6.4 借用值与生命周期

参数模式会自动建立借用；其他位置可用 `borrow expression` 和 `borrow(mut) expression` 显式建立
借用值，其类型分别写作 `borrow T` 和 `borrow(mut) T`：

```sc
let r: borrow i32 = borrow value
let first(borrow values: Slice(i32)): borrow i32 = { borrow values[0] }
```

函数签名中只有一个输入借用可作为返回来源时，生命周期默认与该输入关联。存在多个可能来源、
借用被保存进结构体、或公开 API 无法唯一推断时，必须显式声明 region 参数：

```sc
let choose('a: region)
  (condition: bool)
  (borrow('a) left: T, borrow('a) right: T): borrow('a) T = {
  if condition { borrow left } else { borrow right }
}
```

region 是编译期参数，不存在于运行时。省略 region 不代表 `'static`；字符串字面量等真正静态数据
由预定义 region `'static` 表示。借用检查首先采用基于最后一次使用的非词法生命周期；诊断必须
指出借用建立点、冲突使用点和借用结束条件。

### 6.5 资源释放与拥有容器

Salicin 默认不使用垃圾回收。拥有值在其作用域结束、被覆盖或容器析构时确定性释放。标准 trait
`Drop` 定义自定义清理；实现 `Drop` 的类型不能实现 `Copy`。用户只能在类型定义所在包中实现其
`Drop`，且编译器保证每个仍处于已初始化状态的值恰好析构一次。

`Copy` 是无方法、无可观察副作用并受编译器验证的 marker trait。用户类型仅在所有字段都实现
`Copy`、自身不实现 `Drop` 时才允许实现 `Copy`；共享借用可 Copy，排他借用不可 Copy。昂贵或
可能失败的复制通过显式 `Clone` 操作完成，不由普通读取隐式触发。

struct、array、enum、部分应用和闭包分别通过 field、constant-index、downcast 与 capture 投影逐子值
初始化，enum 还在 payload 前记录 discriminant；只有所有子值完成才初始化 root。调用的值参数和
field/index base 也先 staging，因此构造中途发生 `return`、`break` 或 uninhabited call 时不会提前
提交最终落点。嵌套 `break` 只转移实际完成的内层值，外层半成品沿退出边进入清理。

常量和动态 array index 在当前“array element 必须 `Copy`”的约束下都按 copy extraction 建模。base
以及动态 index 仍各求值并 staging 一次，但结果只初始化 destination，不消费 base 的 element；运行时
`Index(LocalId)` 不能成为有限静态 move path，cleanup verifier 会拒绝这种 forest。

同一版本为 `CleanupPlan` 加入缓存的 CFG fixed point。每个静态 path 节点分别记录 `may_init` 与
`must_init`：join 对前者取 union、后者取 intersection，不可达 predecessor 不参加；scope-exit edge、
`StorageLive` 与 `StorageDead` 清除 local 状态。验证器在 fixed point 后按 block 内 operation 顺序重放，
检查 `MoveOut`/`Overwrite`/`Transfer` source 和 destination、branch condition 与 return place。
enum discriminant 另外稀疏跟踪 possible variant；只有确定 active 的 downcast 才能访问，字段补回可重组
active variant 与 root，whole-value overwrite 会忘记旧 discriminant，Transfer 两侧 forest 必须兼容。

`TemporaryStorageLiveness` pending 因而删除；`PendingCapability` 继续明确标记 conditional
`MaybeOverwrite` cleanup、borrowed-place mutation、match dispatch/pattern binding transfer，以及
partial application/local closure capture。

构造聚合和调用按从左到右的求值顺序暂存已经完成的拥有字段/实参。后续求值提前退出时，return
cleanup 清理这些暂存值；完整聚合或实际 call 提交后则清 flag，把所有权转给结果或 callee。原生
trap 回归测试证明 scope cleanup 可观察执行，并检查同一 storage 不会重复析构。

标准库提供显式拥有容器，而不是语言内建 GC：

- `Box(T)`：唯一拥有的堆值；
- `Vec(T)`：连续、可增长的唯一拥有序列；
- `Rc(T)` / `Weak(T)`：单线程引用计数；
- `Arc(T)` / `WeakArc(T)`：线程安全引用计数；
- arena 和 tracing GC 可作为库实现，但不会改变核心移动/借用规则。

## 7. 块、闭包与捕获

### 7.1 `do` 与立即调用

`do` 是接受零参数尾闭包并立即调用它的内建函数。闭包的最后一个无分号表达式是返回值；空闭包
返回 `()`：

```sc
let n = do {
  let x = 20
  x + 22
}
```

### 7.2 闭包字面量

```sc
let empty = {}
let thunk = {
  expensive_work()
}
let succ = { (x: i32) ->
  x + 1
}
let curried = { (x: i32)(y: i32) -> x + y }
```

每个花括号表达式都是闭包。没有显式参数前缀时，它是零参数闭包；因此 `{}` 返回 `()`，
`{ expression }` 返回该表达式的值。带参数闭包写 `{ (parameters) -> body }`，并可以像具名闭包
一样声明多个参数组。已经删除多余的 `{ -> expression }` 拼法。

需要立即执行花括号表达式时写 `do { ... }`。它就是 `do` 接受一个零参数尾闭包的调用。
`return` 只返回这个匿名函数，不会越过 `do` 返回外层命名函数。

`do` 对闭包的 effect/color 行是多态的：它不处理也不新增 effect，而是把 `Unsafe`、`async`、
错误传播等要求原样传给调用上下文。`unsafe` 与 `async` 则是对应 color 的 handler/构造函数。

因此：

```sc
let f = {}             // 类型为 (): ()
let deferred = { 42 }  // 类型为 (): i32
let x = do {}          // ()
let y = do { 42 }      // i32
let f(x: i32) = {}     // 具名零结果闭包
```

具名闭包声明只是把闭包参数提升到绑定名称旁边：

```sc
let add(x: i32)(y: i32): i32 = {
  x + y
}

// 语言语义等价于：
let add: (i32): (i32): i32 = { (x: i32)(y: i32) ->
  x + y
}
```

因此具名闭包的 RHS 必须使用花括号；`let add(x: i32) = x + 1` 被拒绝。普通值声明仍直接绑定
表达式，例如 `let answer = 42`。具名声明的花括号不会额外创建一层返回闭包：声明参数就是该
闭包的参数。

花括号也用于若干非表达式语法，它们只是对应构造的分隔符：

| 上下文 | `{ ... }` 的含义 |
|---|---|
| 普通表达式位置 | 零参数闭包或带显式参数的闭包 |
| `let f(...)= { ... }` 或带名签名声明 RHS | 参数提升后的具名闭包声明 |
| `if` / `else` / `while` / `for` / `loop` 后 | 由控制构造惰性执行的主体闭包 |
| `struct` / `enum` / `trait` / `extend` 后 | 声明体 |
| `value match { ... }` | match 分支列表 |
| `do { ... }` | 立即执行并透传 effect 的零参数尾闭包 |

match 分支需要立即执行多条语句时写 `do { ... }`；直接写 `{ ... }` 表示该分支返回闭包。
具名闭包需要返回另一个闭包时嵌套花括号，例如 `let make() = { { 42 } }`。

### 7.3 捕获模式

普通闭包默认按最小权限自动捕获：只读使用为 `borrow`，修改为 `borrow(mut)`，消费为 `move`。
即使外部值实现 `Copy`，普通闭包的只读捕获仍优先共享借用，避免是否逃逸反向改变捕获方式。
可以显式指定整个闭包为移动捕获：

```sc
let task = move { consume(buffer) }
```

`move` 闭包对 `Copy` 外部值复制，对其他外部值移动。逃逸闭包不得捕获寿命不足的借用。

每个闭包具有匿名具体类型，并依据闭包体如何使用捕获实现一种或多种调用 trait：

- `Fn`：可通过共享借用重复调用；
- `FnMut`：调用可能修改捕获，需要闭包位于可变 place；
- `FnOnce`：调用会消费捕获或闭包自身，最多调用一次。

函数签名 `(T): U` 用于声明和约束调用形状；捕获闭包的大小、析构和调用能力属于其匿名具体类型。
高阶函数应以泛型 callable 接收闭包，避免隐式装箱：

```sc
let apply_twice(T: type)(F: type)
  (value: T)
  (borrow(mut) function: F): T
where F: FnMut((move _: T): T) = { function(function(value)) }
```

需要异构存储或动态分派时使用未来标准库的显式 `DynFn`/`BoxFn` 类型，不让裸 `(T): U`
悄悄分配堆内存。

## 8. 结构体、构造与成员

### 8.1 名义结构体

```sc
let A = struct { foo: i32, bar: u32 }
let a = A { foo: 1, bar: 2 }
```

结构体是名义类型。结构体值只能用 braced literal 构造，字段必须具名；不提供内建位置构造器。
所有字段都必须初始化且每个字段只能出现一次。字段顺序不影响语义，公开 API 推荐保持字段名稳定。

需要位置式或简短构造时，声明一个普通同名函数：

```sc
let Pair = struct { left: i32, right: i32 }
let Pair(left: i32, right: i32): Pair = { Pair { left: left, right: right } }

let pair = Pair(40, 2)
```

这个 `Pair(...)` 不具有结构体特权，只是普通函数调用；它可以使用具名参数参与函数重载消歧。

结构体声明可以带有限的编译期选项。当前实现支持：

```sc
let Pixel = struct(derive: Copy) { value: i32 }
```

`derive: Copy` 降低为普通 `Copy` trait 实现；泛型结构体会为每个类型参数生成对应 `T: Copy`
约束。

字段默认模块私有、不可通过不可变绑定修改。若结构体值位于可变绑定中，可修改其可见字段：

```sc
let mut a = A { foo: 1, bar: 2 }
a.foo = 3
```

### 8.2 扩展和关联成员

```sc
extend A {
  let reset(borrow(mut) self)(): () = {}
  let bar = 42
}

a.reset()
A.bar
```

带 `self` 参数的函数是实例方法；不带 `self` 的声明是关联成员。忽略位于开头的编译期参数组后，
`self` 必须独占第一个运行时参数组，并可使用 `self`、`borrow self`、`borrow(mut) self`、
`move self`、`copy self`。其类型隐式为扩展目标 `Self`。实例方法必须再声明至少一个显式运行时
参数组；无其他参数时写空组 `()`。这避免 `a.member` 在字段读取和隐式调用之间产生歧义。

方法调用：

```sc
a.reset()
```

脱糖为先应用接收者参数组，再应用源代码中的显式参数组：

```sc
A.reset(a)()
```

调用处不重复写传递模式；编译器依据方法签名对 `a` 建立借用、复制或移动。不允许仅靠重载使
该选择产生歧义。

扩展成员可以在外层扩展参数之外声明自己的编译期参数。外层参数由接收者的名义类型确定，成员
参数紧跟成员名应用，之后才是显式运行时参数组：

```sc
extend(T: type) Box(T) {
  let view(A: access)(borrow(A) self)(): borrow(A) T = {
    borrow(A) self.value
  }
}

let shared = boxed.view()        // A 默认为 shared
let exclusive = boxed.view(mut)()
```

对关联函数，外层与成员编译期参数组都保留声明顺序；二者可由实参和期望结果共同推断。成员不得
重新声明同名外层编译期参数。

`Self` 在 `extend` 成员的类型和表达式位置都表示当前扩展目标，因此构造器、关联成员、限定方法
调用和 enum pattern 可分别写成 `Self { value: value }`、`Self.member`、`Self.method(self: value)()` 与
`Self.Some(value)`。它也适用于泛型扩展、trait 实现和默认 trait 方法；在 `extend` 外使用表达式级
`Self` 是错误。

### 8.3 泛型结构体

```sc
let Box(T: type) = struct { value: T }
let a = Box(i32) { value: 10 }
let b: Box(i32) = Box { value: 20 }
```

`Box(i32)` 是类型头，随后 `{ ... }` 构造该类型的值。`Box { value: 20 }` 可以从期望类型推断
省略的编译期参数；没有期望类型时写 `Box(i32) { value: 20 }`，或提供一个显式同名函数：

```sc
let Box(T: type)(value: T): Box(T) = { Box(T) { value: value } }
let c = Box(20)
```

类型构造器只在编译期求值，并对实际使用的类型组合单态化。

#### 8.3.1 类型别名与类型构造子

类型别名是透明的编译期绑定，不产生新的 nominal identity：

```sc
let Scalar = i32
let Family(T: type): type = Box(T)
let Constructor: (T: type): type = Box
```

`Family(i32)`、`Constructor(i32)` 与 `Box(i32)` 是同一个类型。最后一种写法把 `Box` 这个
`(type): type` 构造子绑定给 `Constructor`；签名中的参数名也是显式类型应用时可使用的标签。
类型构造子值只存在于编译期，在进入运行时 IR 前完全展开，不能存入运行时变量或转换成函数指针。

编译期参数也可以声明构造子 kind：

```sc
let Use = trait(Self: (Value: type): type) {
  let map(E: effect, A: type, B: type)(
    move self: Self(A),
  )(
    move transform: (A): B with(E),
  ): Self(B) with(E)
}

let Handles(E: (Error: type): effect) = trait {
  let run(T: type)(move action: (): T with(E(i32))): T with(E(i32))
}
```

`(Value: type): type` 是类型构造子 kind，`(Error: type): effect` 是 effect 构造子 kind。
`let TraitName(...) = trait(Self: Kind)` 将名称旁参数保留为真正的 trait 参数；`trait(Self:
Kind)` 单独声明被实现主体的 kind。省略 self kind 时等价于 `trait(Self: type)`。当前实现支持
这类参数出现在 trait self kind、trait 参数和 trait 方法签名中，供 `core.functional` 等标准库
协议表达。匹配 arity 的泛型 nominal 构造子可以实现 `Self` 为构造子 kind 的 trait：

```sc
let Functor = trait(Self: (Value: type): type) {
  let map(E: effect, A: type, B: type)(
    move self: Self(A),
  )(
    move transform: (A): B with(E),
  ): Self(B) with(E)
}

let Carrier(T: type) = struct { value: T }

extend Carrier: Functor{let map(E: effect, A: type, B: type)(
    move self: Carrier(A),
  )(
    move transform: (A): B with(E),
  ): Carrier(B) with(E) = {
    Carrier(B) { value: transform(self.value) }
  }}
```

method implementation 会注册为 generic function template，并由普通模板验证路径检查函数体。
receiver-style constructor trait 方法可以从具体 nominal 实例分派：

```sc
let value = Carrier(i32) { value: 41 }.map(add_one)
```

如果多个 constructor trait receiver method 共享同名 member，仍按命名重载规则用具名参数消歧。
没有 receiver 的 constructor trait associated function 仍可以通过裸构造子调用，例如
实现 `Applicative` 后的 `Carrier.pure(...)`。

trait 声明可以在 `trait` 后、`{` 前写继承约束：

```sc
let Applicative = trait(Self: (Value: type): type)
where Self: Functor(...)
```

这里 `Self: Functor` 使用构造器 subject 规则：当前 trait 的 `Self` 构造子必须也实现
`Functor`；右侧只写 `Functor` 自己的 trait 参数。普通泛型函数也可以接收显式构造子参数并写
constructor trait 约束：

```sc
let keep(M: (Value: type): type, A: type)(move value: M(A)): M(A)
where M: Monad = {
  value
}

let kept = keep(M: Carrier)(Carrier(i32) { value: 42 })
```

柯里化构造子和部分应用的透明类型别名可以作为 HKT 实现目标。例如标准库直接用根模块中的
`Result(Error)` 作为一元构造子，并为它实现 `Functor`、`Applicative` 与 `Monad`：

```sc
extend(Error: type) Result(Error): Monad { ... }
```

关联类型 lowering 和完整构造子方程求解仍是后续语义能力；不能唯一决定时仍通过省略编译期参数组和
命名参数由上下文消歧，不会恢复 `_` 推断占位。

带名称旁参数的形式定义类型族，右侧必须产生具体的 `type`：

```sc
let Wrapped(T: type): type = Box(T)
```

因此 `let Wrapped(T: type): type = Box` 不会隐式补成 `Box(T)`；要直接转发构造子，应写
`let Wrapped: (T: type): type = Box`。别名依赖可以前向引用，但递归别名环是错误。透明转发到
nominal 构造子的别名也能省略编译期参数组，由普通构造实参和期望类型推断。

### 8.4 枚举与封闭和类型

枚举使用与其他类型一致的 `let` 声明：

```sc
let Option(T: type) = enum {
  Some(T),
  None,
}

let Result(E: type)(T: type) = enum {
  Ok(T),
  Err(E),
}

let Shape = enum {
  Circle(radius: f64),
  Rectangle(width: f64, height: f64),
  Point,
}
```

每个 variant 都位于枚举类型的成员命名空间：`Option(i32).Some(1)`、`Shape.Point`。
当期望类型能唯一确定枚举，或 variant 已由 `use` 导入时，可以写短名 `Some(1)`、`None`。
若短名对应多个可行 variant，必须使用限定名，不按声明顺序猜测。

带数据 variant 的构造规则与结构体相同：允许全位置或全标签形式，不允许混用。无数据 variant
是值而不是零参数函数，因此写 `None`，不写 `None()`。枚举的 `match` 必须穷尽所有可达 variant。
命名 payload 字段可像 struct 字段一样写 `pub(package)` 或 `pub`；位置 payload 没有独立标注位，
继承 enum 声明本身的可见性。这使公开的 `Option.Some(T)` / `Result.Ok(T)` 可以跨包构造和匹配，
同时允许公开 enum 的私有命名 payload 只通过定义模块提供的 factory 产生。

递归枚举必须通过拥有或借用容器打断无限布局：

```sc
let List(T: type) = enum {
  Cons(T, Box(List(T))),
  Nil,
}
```

variant 之间必须使用逗号分隔，允许最后一个 variant 后保留尾逗号。一个 variant 的数据字段可以
全部按位置声明或全部命名，不允许混合。

枚举的判别值、字段排列和 niche 优化默认属于私有 ABI；只有显式稳定布局属性才能向 FFI 承诺。
首版不提供可绕过判别检查的裸 `union`，此类能力属于 `unsafe` FFI 设计。

### 8.5 类型与成员命名空间

`struct { ... }` 在字段上下文创建运行时名义数据类型，包括零字段的 `struct {}`；在声明上下文创建
编译期模块。两者在首版都只能直接出现在命名 `let` 声明的右侧，不支持匿名名义类型。

实例字段与固有实例方法共享实例成员命名空间，同名时报错，避免 callable 字段与方法调用产生
歧义。关联成员通过 `A.member` 访问，可以与实例字段同名。多个 trait 提供同名方法且上下文无法
唯一选择时，必须使用完全限定调用 `<A as Trait>.method(a)(...)`。模块不是类型，不能构造、实现
trait 或用 `extend` 重新打开。

## 9. 泛型函数与约束

```sc
let identity(T: type)(value: T): T = { value }
identity(i32)(0)
identity(20)
identity(T: i32)(20)
```

类型参数必须位于运行时参数组之前。调用可以省略任意未显式提供的编译期参数组；编译器从后续
运行时实参和期望返回类型双向推断。无法唯一推断时报错，不根据函数体外的任意隐式转换“猜测”
类型。

括号本身不区分编译期和运行期参数组。编译器按以下规则解释：

1. 带标签且标签属于编译期参数的组，选择对应的编译期参数组；允许只指定其中一部分，例如
   `Result(E: IoError).Ok(value)`。
2. 无标签且所有实参都能解析为类型的组，是显式编译期参数组，例如 `identity(i32)(0)`。
3. 其他组从第一个尚未应用的运行时参数组开始匹配；在 `identity(20)` 中，编译期组被省略。
4. 若源码意图可能含混，使用参数名消歧，不引入方括号或新的关键字。

期望类型也参与推断：

```sc
let value: Box(i64) = Box(10)
let made: Product = make(10)
```

类型位置本身没有运行时实参提供证据，因此泛型类型必须写全，例如 `Box(i64)`。`Box { pointer: _ }`、
`identity(_)(20)`、`identity(T: _)(20)` 和独立表达式 `_` 都是语法错误。数组长度等非类型
编译期参数同样不能写 `_`；需要推断时省略整个编译期参数组，并可用真实的命名参数消歧。

使用 `where` 表达 trait 约束：

```sc
use core.ops.Add

let twice(T: type)(x: T): T
where T: Add(T, Output = T), T: Copy = { x + x }
```

没有约束的泛型函数只能使用对所有 `T` 都成立的操作。

```sc
let duplicate(T: type)(copy value: T): T
where T: Copy, = {
  let first = value
  value
}
```

## 10. Trait 与实现

```sc
let Foo = trait {
  let f(borrow self)(x: i32): i32
}

extend A: Foo {
  let f(borrow self)(x: i32): i32 = { x }
}
```

### 10.1 关联类型

```sc
let Bar = trait {
  let Item: type
}

extend A: Bar {
  let Item = i32
}
```

关联类型通过 `T.Item` 或完全限定形式 `<T as Bar>.Item` 引用。存在歧义时必须使用完全限定形式。

关联类型本身也可以接受编译期参数，从而表达容器重新绑定：

```sc
let Chain = trait {
  let Item: type
  let Rebind(Value: type): type
}
```

约束中的 `Output = T` 是关联类型等式，不是运行时命名实参：

```sc
use core.ops.Add

where T: Add(T, Output = T)
```

### 10.2 泛型 trait 与泛型实现

trait 自身的类型参数写在名称之后，`Self` 表示实现目标：

```sc
let Convert(To: type) = trait {
  let convert(move self)(): To
}
```

泛型实现先声明该实现引入的编译期参数，再写目标类型和可选 trait：

```sc
extend(T: type) Box(T): Display
where T: Display {
  let display(borrow self)(): String = { ... }
}
```

```sc
let Cell(T: type) = struct { value: T }

extend(T: type) Cell(T) {
  let new(move value: T): Cell(T) = { Cell { value: value } }
  let take(move self)(): T = { self.value }
}

let cell = Cell.new(42)
let value = cell.take()
```

实现参数必须能从目标类型、trait 参数或 where 约束唯一决定，防止产生无法选择的自由参数。

### 10.3 一致性规则

`Copy`、`Drop`、`Fn`、`FnMut`、`FnOnce`、运算符协议和 `Future` 是
编译器登记的 lang-item traits，但其声明由匹配工具链版本的 `core` 提供。首版只做静态分派；
trait object 及动态分派留作独立设计，不让 `Foo` 默认同时表示 trait object 类型。

### 10.4 运算符

运算符是 trait 调用的语法糖，例如：

```sc
a + b   // Add.add(a, b)
a == b  // Eq.eq(borrow a, borrow b)
a < b   // 根据 PartialOrd.partial_cmp(borrow a, borrow b) 的四态结果判断
```

使用运算符语法本身不要求导入协议；实现协议、在 `where` 中引用协议或直接调用协议成员时，必须
通过普通模块导入，例如 `use core.ops.Add`。同名的用户 trait 不会获得 `+` 的 lang-item 身份。

运算符优先级和求值顺序由语言固定，trait 只能改变操作含义，不能改变解析方式或短路规则。
`&&`、`||`、赋值、成员访问、普通调用以及 effect 传播不可重载。

运算符是否复制、移动或借用操作数完全由对应 trait 方法签名决定；语言不会为 `+` 或 `==`
另加一套所有权例外。`&&`、`||` 的短路和 `=` 的 place 写入仍由语言直接定义。

首批映射固定如下；trait 的准确泛型参数和关联类型由 `core` 声明：

| 语法 | lang-item trait |
|---|---|
| `+ - * / %` | `Add Sub Mul Div Rem` |
| 一元 `-`、`!` | `Neg Not` |
| `& \| ^ << >>` | `BitAnd BitOr BitXor Shl Shr` |
| `== !=` | `Eq` |
| `< <= > >=` | `PartialOrd` |
| `a[index]` | `Index` / `IndexMut` |
| `+= -= *= /= %=` 等 | 对应 `*Assign` trait |
| `?.`、`??` | `Chain`、`Coalesce` |

`!=`、`<=` 等可以由核心 trait 的基本结果组合，但每个操作数仍只求值一次。用户不能声明新的
运算符 token 或改变优先级。

顺序比较采用显式四态结果，避免用整数约定编码比较结果，也不会把无序错误地当成大于或小于：

```sc
let PartialOrdering = enum { Less, Equal, Greater, Unordered }

let PartialOrd(Rhs: type) = trait {
  let partial_cmp(borrow self)(borrow rhs: Rhs): PartialOrdering
}
```

`<`、`<=`、`>`、`>=` 分别接受 `Less`、`Less | Equal`、`Greater`、
`Equal | Greater`；`Unordered` 对四种运算都得到 `false`。每个表达式只调用一次
`partial_cmp`，并只求值一次左右操作数。

一元协议没有右操作数；它们消费 `self`，并允许通过关联类型改变结果类型：

```sc
let Neg = trait {
  let Output: type
  let neg(move self)(): Output
}

let Not = trait {
  let Output: type
  let not(move self)(): Output
}
```

按位与移位协议同样消费两个操作数，并通过关联类型决定结果；五个协议分别使用
`bit_and`、`bit_or`、`bit_xor`、`shl` 和 `shr` 方法：

```sc
let BitAnd(Rhs: type) = trait {
  let Output: type
  let bit_and(move self)(move rhs: Rhs): Output
}
```

内建整数要求左右操作数类型相同。`>>` 对有符号整数执行算术右移，对无符号整数执行逻辑右移。
移位量为负数或不小于左操作数位宽时，常量表达式产生编译错误，运行时值则确定地 trap；不会把
LLVM 的 poison 行为暴露为语言语义。

因此用户类型的 `!value` 不被限制为返回 `bool`。内建 `!bool` 仍返回 `bool`，内建负号只接受
有符号整数。泛型代码可使用 `T: Neg(Output = T)` 或 `T: Not(Output = U)` 约束。

## 11. 模块

```sc
let Math = struct {
  let zero = 0
  let inc(x: i32) = { x + 1 }
}

let one = Math.inc(Math.zero)
```

模块在语法上使用无数据的 `struct { declarations }`，但在语义上是编译期命名空间，不是可实例化
的零字段运行时结构体：不能构造、移动、比较或作为普通值传递。这样保留“模块是不带数据的结构体”
的统一成员模型，同时避免为命名空间制造运行时值。

模块成员默认私有。`pub(package)` 对当前包公开，`pub` 同时对依赖该包的代码公开：

```sc
pub let Client = struct { ... }
pub(package) let parse_header(text: Str) = { ... }
let validate_internal_state() = { ... }
```

私有成员可由声明模块及其子模块访问。公开声明的签名不能泄漏可见性更低的类型或 trait。
模块不能实现运行时 trait，也不能作为普通值捕获。`extend` 只扩展名义数据类型或实现 trait，
不能重新打开模块；一个显式内联模块的成员必须写在其 `struct { ... }` 声明中。

### 11.1 文件模块

每个 `.sc` 文件都是一个隐式模块，模块路径由 `src` 下的相对路径确定：

```text
src/lib.sc       -> 包的库根模块
src/main.sc      -> 默认二进制根模块
src/bin/tool.sc  -> 名为 tool 的额外二进制根模块
src/net.sc       -> net
src/net/http.sc  -> net.http
```

同一路径不能同时由多个源文件定义。文件不需要 `mod` 声明；构建系统发现当前 target 可达的模块。
文件模块与 `let Math = struct { ... }` 声明的内联模块使用相同的成员访问和可见性规则。

### 11.2 导入

`use` 只建立名称别名，不执行文件、复制声明或改变可见性：

```sc
use net.http.Client
use net.http.{get, post}
use net.http.Client as HttpClient
pub use net.http.Status
```

`self`、`super` 和 `root` 分别从当前模块、父模块和当前包根开始解析。依赖包名位于路径首段。
首版不提供 `*` glob 导入，避免依赖升级静默引入名称冲突。`pub use` 可以构造稳定的公共 API，
但不能把依赖包中的私有或 `pub(package)` 名称重新导出。

同一包内模块可以循环引用类型和函数，因为名称收集先于函数体检查；常量求值和类型布局依赖必须
无环。包依赖图必须无环。

### 11.3 包与项目清单

项目根包含 `salicin.toml`，解析后的依赖版本记录在 `salicin.lock`。建议的最小清单为：

```toml
[package]
name = "hello-salicin"
version = "0.1.0"
edition = "2026"

[lib]
path = "src/lib.sc"

[[bin]]
name = "hello-salicin"
path = "src/main.sc"

[dependencies]
local_util = { path = "../local-util" }
```

默认存在 `src/lib.sc` 时生成库 target，存在 `src/main.sc` 时生成同名二进制 target；清单可通过
`[lib]` 和多个 `[[bin]]` 表显式指定其他入口。包名使用 kebab-case，在源码路径中规范化为
snake_case。edition 固定解析和语义规则，不由所安装编译器静默改变。

`salicin.lock` 对应用必须提交版本控制；库可以提交以保证开发环境复现，但发布库的使用者按自己
的依赖图解析。当前实现只接受本地 `{ path = "..." }` 依赖；路径必须使用 `/` 分隔并相对于声明
它的包。lockfile 记录包名、版本、edition、规范化路径和完整依赖边；能相对于根包表示时写相对
路径，跨文件系统根时保留规范化绝对路径。本地来源没有校验和。registry 版本范围、Git 来源、
校验和及多版本求解留给后续包管理切片。首版不支持在构建期间执行任意代码的 build script；
本地生成步骤由外部构建工具显式完成。构建产物默认写入 `build/`，不与源码混放。

### 11.4 程序入口与退出

每个二进制 target 必须恰有一个非泛型、零参数组入口：

```sc
let main(): i32 = { 0 }
```

其返回类型必须实现标准库的 `Termination` trait。M0 只内建 `()`（退出码 0）和 `i32`；标准库
随后为 `Result(E)(())` 提供实现，要求 `E: Display`。不为 `Future(T)` 隐式选择执行器；异步程序
在同步 `main` 中显式调用 `std.async.block_on`。命令行参数和环境通过 `std.env` 显式读取，不进入
平台相关的 `main` ABI。`pub main` 不会自动导出 C 符号；源码入口、模块可见性与链接器导出是
三个独立概念。

## 12. 模式匹配

关键字固定为 `match`（原示例中的 `march` 视为拼写错误）：

```sc
value match {
  Some(x) => x,
  None => 0,
}
```

`match` 是表达式，所有可到达分支必须有可统一的类型，并对封闭类型做穷尽性检查。首版模式包括：

- `_` 通配模式；
- 字面量与范围模式；
- 名称绑定；
- 结构体和枚举/variant 解构；
- 元组模式；
- `p | q` 或模式；
- `pattern if condition` 守卫；
- `borrow name`、`borrow(mut) name`、`move name` 显式绑定模式。

匹配拥有的值默认遵循普通读取规则：`Copy` 字段复制，否则移动；匹配借用值时绑定默认为借用。
守卫只允许观察绑定，不应在分支确定前消费它们。

### 12.1 `match` 的位置与分支规则

Salicin 固定采用后缀 `match`：先写被检查表达式，再写 `match` 和分支。所谓“与 Rust 相同”指
pattern 的解构、穷尽和所有权规则相近，不表示关键字位置相同。被检查表达式只求值一次。

```sc
compute() match {
  Ok(value) if value > 0 => value,
  Ok(_) => 0,
  Err(error) => throw(error),
}
```

分支从上到下测试；有守卫的 pattern 即使覆盖某个 variant，也不计入无守卫的穷尽覆盖。`|` 两侧
必须绑定相同名称，并为每个名称产生相同类型和传递模式。不可达分支默认产生编译警告。

## 13. 控制流

### 13.1 条件表达式

条件必须是 `bool`，不提供整数、指针或容器的隐式 truthiness：

```sc
let sign = if value < 0 {
  -1
} else if value > 0 {
  1
} else {
  0
}
```

有 `else` 的 `if` 是值表达式，各可达分支类型必须能统一。无 `else` 的 `if` 类型固定为 `()`，
其 then 块也必须产生 `()`。条件解构可写：

```sc
if let Some(value) = option {
  use(value)
}
```

`if let` 不承担穷尽检查，绑定只在 then 块可见；需要处理全部情况时使用 `match`。

在 `if`、`while` 和 `for` 的最外层控制头中禁用尾随闭包，第一个未被括号包围的 `{` 总是
控制流主体。条件本身需要尾随闭包时必须加括号：

```sc
if (validate(input) { (error: Error) -> log(error) }) {
  continue_work()
}
```

### 13.2 循环

```sc
loop {
  if ready() { break result }
}

while condition {
  step()
}

for item in collection {
  consume(item)
}
```

`loop` 是表达式，其类型由所有带值 `break` 的值统一；没有可达 `break` 时类型为 `Never`。
`while` 和 `for` 的类型固定为 `()`，其中的 `break` 不能携带非 `()` 值。`continue` 开始下一次
迭代。带标签的多层跳转暂不进入核心语法。

`for pattern in expression` 通过标准库 `IntoIterator`/`Iterator` trait 展开，被迭代表达式只求值
一次。pattern 每次迭代重新绑定，其 move/borrow 行为由迭代器的 `Item` 类型决定。

两个协议位于 `core.iter`，不属于 prelude：`IntoIterator` 声明关联类型 `IntoIter` 和消费
`self` 的 `into_iter`；`Iterator` 声明关联类型 `Item` 和可变借用 `self` 的 `next`，返回
`Option(Item)`。实现和约束中显式命名协议需要普通 `use`，但 `for` 语法直接绑定经过工具链校验的
lang-item 身份，同名 inherent 方法或其他 trait 不能截获展开。当前实现先接受名称绑定与 `_` 这两种
显然不可失败的 pattern；结构解构将在 irrefutability 检查覆盖名义结构后开放。

### 13.3 函数退出、赋值与不可恢复失败

`return expression` 立即退出当前命名函数或当前闭包，类型为 `Never`。省略表达式等同
`return ()`。函数体最后的表达式是隐式返回值，但不会隐式包装进任意用户类型。

普通赋值及 `+=` 等复合赋值的类型均为 `()`。复合赋值只求值一次左侧 place，并通过对应的
赋值 trait（例如 `AddAssign`）实现；它不是简单的文本改写 `x = x + y`。

复合赋值协议为 `AddAssign`、`SubAssign`、`MulAssign`、`DivAssign`、`RemAssign`、
`BitAndAssign`、`BitOrAssign`、`BitXorAssign`、`ShlAssign`、`ShrAssign`，分别对应 `+=`、`-=`、
`*=`、`/=`、`%=`、`&=`、`|=`、`^=`、`<<=`、`>>=`。协议方法可变借用 `self`、消费 `rhs`
并返回 `()`；语法直接绑定 `core.ops` 中经过验证的 lang item。同名 inherent 方法或其他 trait
不参与选择。

可恢复失败使用 `Throws(E)`，需要把失败保存为值时由 `try { ... }` 产生 `core.Result`。首版 panic
策略固定为终止进程（abort），用于数组越界、违反断言等无法在当前 API 中恢复的错误；不进行栈展开，
也不允许 panic 穿过 C ABI。

## 14. 可空类型与条件传播

`Option(T)` 和 `Result(E)(T)` 是 `core` 根模块中的普通 enum。语言为 `?.` 与 `??` 提供标准协议。

### 14.1 可选链 `?.`

```sc
user?.address?.city
result?.normalize()
```

`?.` 对成功分支执行后续成员访问或调用，对空/错误分支保持原容器并跳过后续操作。其协议为
`Chain`，使用泛型关联类型表达“换掉成功值、保留容器形状”：

```sc
let Chain = trait {
  let Item: type
  let Rebind(Value: type): type

  let chain(E: effect, U: type)
    (move self)
    (move transform: (Item): U with(E)): Rebind(U) with(E)
}
```

- `Option(T)?.f` 得到 `Option(U)`；`None` 保持 `None`。
- `Result(E)(T)?.f` 得到 `Result(E)(U)`；`Err(e)` 保持 `Err(e)`。

编译器把 `value?.member` 的后续操作构造成传给 `Chain.chain` 的闭包。链中操作若自身返回同类容器，
默认不自动展平；需要显式 `flat_map`。标准 `Option` 和 `Result` 的实现消费左侧容器，确保失败
residual 只移动或析构一次；借用容器可由标准库提供单独的 `Chain` 实现。

当前实现支持 concrete 与 generic nominal trait impl 中的直接构造子绑定，例如
`let Rebind = Maybe`，并能让非 `Option`/`Result` nominal 类型的 `?.` 调度到 `Chain.chain`。
自定义 `?.` 的 transform 目前必须能降为无捕获 lifted function；会捕获外层参数的可选方法调用仍等待
通用 callable-to-function bridge。

### 14.2 合并运算符 `??`

```sc
let port = configured_port ?? 8080
let data = read() ?? empty_data
```

`??` 在左侧成功时取出 `T`，否则惰性计算右侧 `T`。它通过 `Coalesce` trait 实现，编译器把右侧
包装为零参数闭包；闭包是否为一次性调用由普通捕获/移动规则推断，所以右侧严格按需执行。结果类型为
`T`：

```sc
let Coalesce = trait {
  let Item: type
  let coalesce(E: effect)
    (move self)
    (move fallback: (): Item with(E)): Item with(E)
}
```

- `Option(T) ?? T` 的结果为 `T`
- `Result(E)(T) ?? T` 的结果为 `T`

若需要使用错误值恢复，调用标准库方法：

```sc
let value = result.recover { (error: Error) -> fallback(error) }
```

`recover` 是普通方法，不是语言语法；它通过独立参数组接收尾随闭包。

`?.` 与 `??` 和其他可重载运算符一样允许用户类型实现，但实现必须满足 `Chain`/`Coalesce`
lang-item trait。标准 `Option`/`Result` 实现提供语言定义的短路行为；用户实现的短路律属于协议契约
和标准库测试约束，不由当前类型系统证明。

## 15. 错误 effect 与 `try` handler

```sc
use core.effects.Throws

let load(path: Path): Document with(Throws(IoError)) = {
  let bytes = read_file(path)
  parse(bytes)
}
```

完整调用的签名若含 `Throws(E)`，调用表达式的成功类型就是函数的逻辑返回类型，错误会自动传播到
最近的兼容边界。调用点不写 `.try`；这使错误传播与异步挂起、UI/composable 等其他 effect 使用
同一套规则。部分应用尚未执行函数体，因此不会触发传播。

传播只接受完全相同的错误类型。语言不会隐式把 `E1` 转换为 `E2`；需要合并错误集合时，应由
调用者显式映射到一个共同的 enum，或由库函数提供转换：

```sc
let load(): Document with(Throws(AppError)) = {
  read_file().map_error { (error: IoError) -> AppError.io(error) }
}
```

`throw(error)` 产生当前 `Throws(E)`，要求 `error: E`，其表达式类型为 `Never`。`return value` 只表示
函数的正常完成，不接受 `Result` 作为隐式传播通道。

### 15.1 处理错误

`try` 接受尾闭包并处理其中的 `Throws(E)`：

```sc
use core.Result

let result: Result(IoError)(Document) = try {
  load(path)
}

result match {
  Ok(document) => show(document),
  Err(error) => report(error),
}
```

正常尾值产生 `Ok(value)`，未在内部处理的错误产生 `Err(error)`。`try` 只处理 `Throws(E)`，不处理同一
闭包中的 `unsafe`、异步或用户 marker；这些要求继续留在外围 row 中。绑定、返回值或实参提供的
`Result(E)(T)` 可以确定 handler 类型；没有这类上下文时，编译器从闭包内唯一逃逸的 `Throws(E)` 来源
反向推断直接函数调用和局部函数值调用。多错误类型、`Never`-only action 和部分泛型 residual-row 场景仍必须显式转换或用上下文消歧。

`do { ... }` 不处理错误，只转发闭包的完整 effect row。因此它不能替代 `try`；它的作用仍是创建并
立即调用一个普通尾闭包边界。即使块内的 `return` 要求生成独立 closure，`Throws(E)`、
`unsafe` 和用户 marker 也会穿过该边界继续由外层处理。

### 15.2 `Result`、`Option` 与库协议

`Result(E)(T)` 与 `Option(T)` 是 `core` 根模块中的普通 enum。`??`、`?.`、`match` 和普通方法操作
这些值，但不会让调用自动传播，也不会隐式包装函数尾值。返回这些类型必须显式构造 `Ok`、`Err`、
`Some` 或 `None`。
旧的 `Try`、`FromResidual` 与 `FromError` traits 已删除。当前实现中用户类型还不能接管 `throw`
或调用传播；后续若开放该能力，应通过标准库 trait/protocol 接入，并让标准 `Throws(E)` 走同一套契约。

后缀 `.try`、lowercase `with(throws(E))`、`with(try)`、`with(try(E))` 和 `try do`
均已在 1.0 前移除，不提供兼容别名。

## 16. 异步

```sc
let f(x: i32): i32 with(Async) = {
  let a = foo()
  a + x
}
```

异步是 effect row 中的控制 effect，不由返回类型暗示，也没有逐调用的 `.await`。在异步上下文中
完整调用 `foo()` 本身就是挂起点，并自动把异步 effect 传播到当前边界。`async { body }` 处理该
effect，生成唯一的匿名状态机类型；该类型实现 `Future(Output = T)`。局部变量可以持有推断出的
具体 Future；高阶 API 以泛型约束接收：

```sc
let run(T: type)(F: type)(move future: F): T
where F: Future(Output = T) = { ... }
```

需要异构存储或隐藏于非返回位置时，显式使用标准库的 `BoxFuture(T)`。这样普通异步调用不要求堆
分配或动态分派。

`f(1)` 在另一个 `with(Async)` 边界中是逻辑类型 `i32` 的挂起调用；在 `async { f(1) }` 中则被
handler 捕获并形成冷 Future，在首次轮询前不执行函数体。多参数组函数只有在最后一组应用后才
产生 effect；之前仍是普通部分应用闭包。

异步闭包和立即异步 handler 分别写：

```sc
let task = { fetch() }                       // 零参数闭包，类型含 with(Async)
let immediate = async { task() }             // handler 构造 Future
```

异步 handler 编译成状态机；跨挂起调用存活的局部值成为状态机字段。借用外部输入可以跨挂起点，
相应 region 会成为 Future 类型的一部分，并限制 Future 的逃逸范围。首版拒绝让 Future 借用自己
的其他状态字段并跨挂起点，以避免自引用状态机；底层 poll API 通过 `Pin` 保证已开始轮询的状态机
不再移动。直接递归异步调用形成无限状态机，必须通过 `BoxFuture` 等显式间接层打断。

丢弃尚未完成的 Future 表示取消：按已初始化状态析构所有字段并释放所持借用，不会让任务在后台
继续执行。语言不隐式选择线程池或事件循环；执行器属于 `std.async`，二进制入口若要运行 Future
必须显式调用执行器。

多个 handler 的次序决定嵌套 carrier。例如：

```sc
let future_of_result = async { try { fetch() } }
let result_of_future = try { async { fetch() } }
```

两者分别具有 `Future(Result(E)(T))` 与 `Result(E)(Future(T))` 的形状（后者只有当错误在构造 Future
时同步产生才有意义）。handler 不做隐式交换或合并，因此控制流和取消边界保持可读。

## 17. 求值顺序和副作用

- 函数位置先求值，实参按源码从左到右求值。
- 二元运算符先左后右；`&&`、`||`、`??` 短路。
- 结构体字段按构造表达式出现顺序求值，而非声明顺序。
- 赋值先求值目标地址，再求值右侧，再写入。
- 析构按局部绑定初始化的逆序发生；部分移动值只析构仍拥有的字段。

固定求值顺序是语言保证，LLVM 优化不得改变可观察行为。

## 18. 类型推断与重载

推断以局部、可预测为目标：

1. 函数参数类型必须显式写出，闭包参数仅在调用上下文唯一时可省略。
2. 私有函数返回类型可推断；递归或公开函数必须显式标注。
3. 省略的泛型编译期参数组从实参和期望结果双向推断；命名实参用于消歧。
4. 不做跨任意数量隐式转换的重载搜索。
5. 数字扩宽、Option 包装等默认不隐式发生；使用显式转换或构造器。
6. trait 解派必须在编译期唯一。

同一作用域中的具名函数可以重载，但候选的完整运行时参数标签组必须不同：

```sc
let open(path: Path): Document = { ... }
let open(url: Url): Document = { ... }

let local = open(path: config_path)
let remote = open(url: endpoint)
```

调用重载名时至少有一个实参必须具名；所有已提供的柯里化参数组一起筛选候选，恰好剩下一个才
成功，因此区分标签位于后续组时也可以在完整调用处消歧。已足以区分候选之外的组仍可使用位置
实参。部分应用只有在当前已提供组已经唯一选择候选时才成立；否则继续提供区分组完成调用，或写
显式适配闭包。裸重载名不能直接作为函数值。

参数类型、传递模式、effect 和返回类型都不参与重载身份或候选搜索；尤其不支持仅按类型或返回
类型重载。相同标签形状的声明是重复定义。这样重载选择只依赖调用的稳定表面形状，不与双向类型
推断、隐式转换、默认传递模式或 effect 子类型形成循环。

inherent 方法与关联函数遵循相同规则。方法的隐式 `self` 接收者组不算消歧证据；候选必须由调用
处显式参数组中的具名参数选择。实例调用 `value.open(path: p)` 与限定调用
`Type.open(self: value)(path: p)` 选择同一候选。

trait 可以声明同名 requirement，只要完整运行时参数标签形状不同。实现必须为每个 requirement
提供相同标签形状的成员；默认实现也属于对应形状。trait 方法和 trait 关联函数的调用仍由显式
具名参数选择，且可以用标签消除多个可见 trait 提供同名成员时的歧义。实现匹配与调用选择都不
读取参数类型、返回类型或 effect。

泛型函数参与同一个重载模型。编译器先只用运行时实参标签选出唯一模板，然后才在该模板内部
处理显式编译期参数组或执行上下文推断。例如 `choose(right: value)` 与
`choose(i32)(right: value)` 都由 `right` 选择 overload；`i32` 以及具名编译期参数都不参与
候选筛选。具体 nominal 上声明的泛型 inherent 成员遵循相同顺序。
blanket generic inherent extension 中的同名成员也按运行时标签形状组成重载集；每个满足 extension
约束的具体 nominal 实例得到同一组候选。extension 的类型参数和 `where` 条件不参与重载选择。

核心隐式 coercion 仅包括 `Never` 到任意类型、`borrow(mut) T` 到 `borrow T`，以及明确登记的
unsizing（例如借用固定数组得到 `borrow Slice(T)`）。整数之间不隐式扩宽，容器不隐式包装，
用户自定义转换也不参与重载搜索。

无损转换使用 `From`/`Into`，可能失败的转换使用 `TryFrom`，明确的位级重解释只能在 `unsafe`
API 中进行：

```sc
let wide = i64.from(value)
let narrow: Result(RangeError)(i32) = i32.try_from(wide)
```

类型名形似调用的语法只调用真实构造器，不自动赋予截断或符号改变语义。

## 19. LLVM 与 ABI 设计边界

这些是实现策略，不属于可观察语义：

- 泛型函数和泛型结构体单态化。
- 不捕获闭包可降低为函数指针；捕获闭包降低为环境结构体加调用函数。
- 柯里化的部分应用使用同一闭包表示。
- trait 静态分派在单态化后直接调用；动态 trait object 若加入则使用显式 witness/vtable。
- `async { ... }` 降低为状态机；其中具有 async effect 的完整调用是状态转换点。
- `Option` 可以利用 niche 优化，但布局只在显式稳定 ABI 属性下成为承诺。

默认 ABI 是语言私有 ABI，可随编译器版本变化。与 C 互操作应使用显式 `extern "C"`，并限制为
C 可表示的签名和布局。

### 19.1 `unsafe` 与 C FFI

内存安全核心之外的操作必须显式进入 `unsafe { ... }` handler 或 `: T with(Unsafe)` 函数。表面上的
`unsafe` 与 `do` 一样接受尾闭包，但它会处理闭包要求的 `Unsafe` effect；`do` 只负责透传该 effect。
`unsafe` 允许调用者承担编译器无法证明的前置条件，但不会关闭普通类型检查、借用检查或可见性检查。

```sc
@repr(C)
pub let Point = struct { x: f64, y: f64 }

extern "C" {
  @link_name("puts")
  let c_puts(text: Ptr(c_char)): c_int
}

@export_name("salicin_add")
pub extern "C" let add(a: c_int, b: c_int): c_int = { a + b }
```

`Ptr(T)` 和 `MutPtr(T)` 是不受借用检查器保护、可以为空的原始指针。解引用、指针算术和调用导入的
C 函数需要 `unsafe`。`core.ffi` 提供 `c_char`、`c_int`、`c_long` 等平台 C 类型；Salicin
`char` 是 Unicode scalar，不能代替 C `char`。

```sc
let pointer = unsafe { raw_alloc(T)(size: bytes, align: alignment) }
unsafe { raw_dealloc(pointer: pointer, size: bytes, align: alignment) }
```

`raw_alloc` 返回非空 `MutPtr(T)`，失败或非法 layout 会终止进程；若期望类型是 `MutPtr(T)`，类型组可
省略。`raw_dealloc` 从 `MutPtr(T)` 推断 `T`，也允许显式写出。`size` 与 `align` 是 `u64`，alignment
必须是非零二次幂；释放必须传回创建 allocation 时完全相同的 layout。使用已释放指针、重复释放、
错误 layout 或访问未初始化内存均属于调用者在 `unsafe` 边界内承担的责任。

LLVM 私有 lowering 调用 `salicin_alloc(i64, i64) -> ptr` 与
`salicin_dealloc(ptr, i64, i64) -> void`。`salic build/run` 链接弱默认实现，平台运行时或嵌入程序可用
同 ABI 的强符号替换；`emit-ir` 保留未解析声明。这个 ABI 不承诺 Salicin 普通函数的名称修饰或调用
约定，只承诺上述两个运行时符号。

布局查询可以在函数表达式中参与普通 `u64` 运算，也可以单独作为顶层常量初始化器。函数类型及错误
恢复类型没有可查询的数据布局。拥有容器的 API 属于 `alloc`，不构成语言语义的一部分。

C ABI 只允许标量、原始指针、C ABI 函数指针和 `@repr(C)` 聚合。C 函数只有一个参数组，
不允许柯里化、泛型、闭包环境、trait、Future 或 Salicin 私有容器；`borrow` 不跨 ABI，必须转换为
显式指针。普通 `bool`、`String`、slice、`Option`、`Result` 默认都不是 C ABI 类型。

`@repr(C)` 固定字段顺序并采用目标平台 C 布局。`extern "C"` 是函数类型的一部分。`pub` 只控制
Salicin 名称可见性；只有 `@export_name` 等显式属性建立稳定链接符号。panic 不得越过 FFI 边界，
一律 abort。C++ ABI、C 可变参数和外部可变全局不在本文定义范围内。

### 19.2 标准库边界

标准库分为三层保留包：

- `core`：不依赖堆或操作系统，包含语言 trait、基础类型、`Option`、`Result`、slice、迭代器、
  `Future`/`Poll`、原始指针等；
- `alloc`：依赖可替换分配器，包含 `Box`、`Vec`、`String`、`Rc`、`Arc` 和集合；
- `std`：依赖宿主系统，包含 IO、文件、路径、环境、进程、时间、线程、同步、网络和异步执行器。

prelude 是按 edition 固定的一小组隐式导入，仅容纳普遍需要的语言基础名称。运算符 trait、拥有容器
和宿主 API 属于普通模块，使用 `use core.ops...`、`use alloc...` 或 `use std...` 显式引入。升级标准库
不能向旧 edition 静默加入产生冲突的 prelude 名称。
编译器登记的 lang-item 声明必须来自版本匹配的 `core`。无标准库目标可以只链接 `core` 和最小
运行时；是否启用 `alloc`/`std` 由 target 与项目清单决定。LLVM IR/bitcode 和 Salicin 私有 ABI
都不是稳定的跨编译器版本发布格式。
