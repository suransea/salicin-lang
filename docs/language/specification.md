# Salicin 语言规范

状态：演进中的设计规范  
目标：静态类型、静态编译、LLVM 后端、默认内存安全、支持所有权与柯里化。  
本文中的语言名称为 **Salicin**。

本文定义“源程序是什么意思”，不把 LLVM 的实现限制暴露成语言规则。实现状态与未完成工作记录在
[项目状态](../project/status.md)，版本变化记录在[更新日志](../../CHANGELOG.md)。Salicin 源文件统一
使用 `.sali` 后缀。

## 1. 设计原则

1. `let` 是统一的不可变名称绑定语法，可以绑定值、函数、类型、trait 和模块。
2. `let mut` 只建立可重新赋值的值绑定，不允许用来改变类型、函数、trait 或模块。
3. 所有表达式都有类型；无结果表达式的类型是 `()`，prelude 中的 `void` 只是该类型的普通别名。
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
- Salicin 源文件使用 `.sali` 后缀；UTF-8 是唯一源码编码。
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
- region 名以 `'` 开头，后接普通标识符主体，例如 `'a`、`'input`；`'static` 是预定义 region。

## 3. 声明与作用域

### 3.1 不可变和可变绑定

```sali
let answer = 42
let answer: i32 = 42
let mut count = 0
count = count + 1
```

`let` 建立不可重新赋值的绑定。不可变约束作用于绑定，而不自动承诺其引用对象不存在
内部可变性。`let mut` 允许重新赋值，但新值必须与绑定的静态类型相同。

同一词法作用域不允许重复声明同名绑定；内层作用域可以遮蔽外层绑定：

```sali
let x = 1
do {
  let x = "one" // 合法，遮蔽外层 x
}
```

变量必须在使用前完成初始化。不提供“声明但未初始化”的安全语法。

### 3.2 顶层声明类别

名字位于不同的语义类别，但名称查找使用同一命名空间，避免 `A` 同时指类型和值：

```sali
let n = 1                         // 值
let add(x: i32)(y: i32) = x + y // 函数值
let Point = struct(x: i32, y: i32) // 类型
let Display = trait { ... }       // trait
let Math = struct { ... }         // 模块
```

顶层值必须能在编译期初始化；需要运行期初始化的全局状态应由显式初始化函数或惰性容器
提供。普通模块级 `let mut` 被禁止；共享可变状态使用 `Atomic`、`Mutex` 等安全容器，或声明
`unsafe let mut` 并在每次访问时承担同步与别名责任。编译期顶层值不依赖声明顺序，但其常量求值
依赖图必须无环。

`let` 右侧的 kind 决定绑定类别：`let Index = usize` 建立类型别名，`let n = 1` 建立值。
同一名字仍不能跨类别重复。具名函数在自身函数体中可见以支持递归；普通值绑定在 initializer
完成前不可见。

### 3.3 可见性

声明默认对所在模块及其子模块可见。`pub(package)` 对当前包公开，`pub` 对所有依赖者公开：

```sali
pub let f(x: i32) = x
pub let Point = struct(pub x: i32, pub y: i32)
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

`void` 和 `never` 不是额外的原始类型；edition prelude 等价于包含以下普通声明：

```sali
let void = ()
let never = enum {}
```

其中 `void` 是 `()` 的类型别名，没有独立的类型身份或 ABI；源码可按偏好使用二者。`never` 是
零 variant 枚举，因此没有任何值。`return`、`throw`、无可达 `break` 的循环和其他不终止表达式
具有 `never` 类型，并可强制转换到任意期望类型。用户声明的其他零 variant 枚举同样是
uninhabited type；对其做空 `match {}` 会产生 `never`。

当前引导实现已经从普通 `core` 源解析 `never`；在通用类型别名 item 落地前，parser 仍把 `void`
直接规范化为 `()`。这是引导期实现限制，不改变上述语言语义。

整数文字先作为“未定整数”参与推断；若上下文没有约束，默认 `i32`。有符号整数溢出在
debug 构建中检查，release 构建默认二进制补码回绕；可另行提供显式 checked/wrapping API。
内建整数 `/` 与 `%` 在除数为零时 trap；对有符号整数，`MIN / -1` 与 `MIN % -1` 也 trap，避免
进入 LLVM 的未定义算术。编译期常量求值会直接拒绝这些情况，而不是生成只能在运行期失败的值。

类型构造使用普通调用外形：

```sali
Option(i32)
Result(i32, IoError)
Future(i32)
A(i32)
```

`_` 不参与任何类型或编译期参数推断，也不是类型或表达式；类型参数、值参数、region 参数及其
命名实参均不能用 `_` 占位。泛型调用通过省略编译期参数组触发推断；`_` 只保留在模式通配符和
匿名函数类型槽等本来就表示“忽略名称”的位置。

### 4.1 复合类型与字面量

首批核心复合类型为：

```sali
(i32, String)       // 元组
Array(i32, 4)       // 固定长度数组；长度是编译期 usize
Slice(i32)          // 连续元素的非拥有视图
Str                 // 不可变 UTF-8 字符串视图
String              // 拥有的 UTF-8 字符串
```

对应字面量：

```sali
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

```sali
let f(x: i32) = {}
let fc(x: i32)(y: i32): i32 = x + y
let unit(): () = ()
```

每一对括号形成一个参数组。`fc` 的抽象类型写作：

```sali
(i32): (i32): i32
```

函数签名允许保留参数名：

```sali
(x: i32): (y: i32): i32
```

参数名用于文档、诊断，以及在函数声明中把名字引入函数体，但不参与类型相等性。因此
`(x: i32): i32` 与 `(value: i32): i32` 是同一函数类型。`->` 不用于函数类型，它只分隔
闭包字面量的参数和闭包体。因此：

```sali
let f(x: i32) = {}
```

正是下式的简写形式：

```sali
let f: (x: i32): void = {}
```

后一种形式是独立的“带绑定名函数签名声明”，不是普通值类型标注的文本脱糖：每个参数槽都必须
有名字，右侧必须是函数体，这些名字会在函数体中可见。普通函数值绑定使用无名签名和显式闭包：

```sali
let succ: (i32): i32 = { (n: i32) -> n + 1 }
```

所以两种函数声明在类型和行为上等价，但 parser 会保留其不同来源以提供正确作用域和诊断。

省略返回类型时从函数体推断。公开 API 建议强制写返回类型，递归函数必须写返回类型。

### 5.2 柯里化

调用一次只消费一个参数组：

```sali
let add(x: i32)(y: i32) = x + y
let add_one = add(1) // 类型为 (i32): i32
let three = add_one(2)
let also_three = add(1)(2)
```

`f(a, b)` 是一个包含两个参数的参数组；`f(a)(b)` 是两个各含一个参数的组，二者类型不同。
部分应用产生闭包，其环境保存已经传入的参数。保存方式服从参数的传递模式：`copy` 复制、
`move` 转移、`borrow` 借用。

零参数组 `()` 不是多余语法。它表示显式延迟调用：

```sali
let make_logger(config: Config)(): Logger = ...
let logger = make_logger(config)()
```

### 5.3 命名实参

函数、方法、闭包和构造器都允许按位置或按名称传参；同一参数组不能混用两种形式：

```sali
make(value: 10)
subtract(left: 44, right: 2)
```

运行时命名实参使用声明中的参数名，并按参数声明顺序书写，因此仍保持实参从左到右求值。参数名
一旦用于外部调用就属于源代码 API。编译期命名实参还可只给出一组中的部分参数，以消除省略组和
运行时组之间的歧义；未给出的参数继续由上下文推断。

### 5.4 尾随闭包

```sali
let value = f(x) { (n: i32) -> n + 1 }
```

严格脱糖为：

```sali
let value = f(x)({ (n: i32) -> n + 1 })
```

尾随闭包总是新建一个只含该闭包的参数组，不会加入前一组。所以下面两种调用不等价：

```sali
f(x) { (n: i32) -> n + 1 }    // f(x)({ ... })
f(x, { (n: i32) -> n + 1 })   // f(x, { ... })
```

接收尾随闭包的函数应把闭包放在独立的最后参数组中：

```sali
let map(T: type)(U: type)
  (items: List(T))
  (transform: (T): U): List(U) = ...

let names = map(T: User)(U: String)(users) { (user: User) -> user.name }
```

一条调用表达式只允许一个尾随闭包。需要传递多个闭包时，其余闭包使用普通参数组显式传入。
尾随闭包必须在同一逻辑行紧跟一个已经含显式参数组的调用，所以允许 `f(x) { ... }`，不允许
`f { ... }`。尾随闭包之后仍可继续成员访问或普通调用。

### 5.5 函数类型与应用时机

函数类型中的冒号右结合：

```sali
(i32): (i64): bool
```

解析为 `(i32): ((i64): bool)`。参数组数量和每组 arity 都是类型的一部分，因此
`(i32, i64): bool` 与 `(i32): (i64): bool` 不是同一类型。参数传递模式也属于函数类型，
参数名则不属于：

```sali
(copy _: i32): bool
(move _: i32): bool       // 与上一类型不同
(value: i32): bool        // value 不参与类型相等性
```

匿名槽中的显式模式必须写 `_:`，以区分“借用一个 `T`”和“按默认模式传递一个已有借用值”：

```sali
(borrow _: T): U       // 参数模式是 borrow，参数底层类型是 T
(_: borrow T): U       // 参数模式是 auto，参数值类型本身是 borrow T
```

命名函数和闭包都是一等值，可以保存、作为参数传递或返回。每次应用参数组都会调用当前函数层，
并立即从左到右求值该组实参。多参数组声明是嵌套函数层的简写，其源码函数体属于最内层；外层
只完成参数绑定并返回下一层，因此在最后一组应用前不会执行该源码函数体：

```sali
let f(x: Resource)(y: i32) = use(x, y)
let pending = f(resource) // resource 此处已按参数模式移动或借用；函数体尚未执行
let result = pending(1)   // 此处进入函数体
```

但显式返回闭包的单组函数可以在第一组调用时执行代码：

```sali
let make_adder(x: i32): (i32): i32 = do {
  log("creating adder")
  { (y: i32) -> x + y }
}
```

部分应用结果是编译器生成的闭包。连续应用可以在不改变上述可观察行为的前提下优化为直接调用。
函数值比较没有内建语义，不自动实现 `Eq` 或 `Hash`。

## 6. 参数传递与所有权

```sali
let f(
  copy a: i32,
  move b: Buffer,
  borrow c: Document,
  mut borrow d: Canvas,
) = {}
```

传递模式定义如下：

| 模式 | 调用效果 | 函数体能力 |
|---|---|---|
| `copy T` | 复制实参；要求 `T: Copy` | 拥有独立值 |
| `move T` | 转移所有权；原绑定之后不可用 | 拥有值 |
| `borrow T` | 建立共享借用 | 只读访问 |
| `mut borrow T` | 建立排他借用 | 可变访问 |
| 未标注 | `T: Copy` 时为 `copy`，否则为 `move` | 同对应模式 |

`mut borrow` 是一个不可拆分的传递模式，`mut` 修饰借用能力，而不是重新绑定参数。

核心借用规则：

1. 任意时刻可以存在多个共享借用，或一个排他借用，但不能同时存在。
2. 借用不能长于其来源。
3. 移动后绑定不可再使用；给该可变绑定重新赋值后可再次使用。
4. 部分移动后只允许访问尚未移动的字段。
5. `Copy` 类型的普通读取不发生移动。
6. 返回值和闭包捕获都参与生命周期检查。

借用检查采用基于最后一次使用的非词法生命周期。局部生命周期尽量推断；跨结构体保存借用或
公开签名无法唯一推断来源时，使用 6.2 节的显式 region 参数。

### 6.1 显式模式优先于类型默认值

显式 `copy`、`move`、`borrow` 和 `mut borrow` 永远优先于默认规则。特别地，即使 `i32`
实现了 `Copy`，传给 `move value: i32` 也会在语言语义上使调用方的原绑定失效。优化器可以消除
机器层面的复制，但不能让已移动绑定重新可用。这使泛型 API 能明确表达“消费一次”的协议。

`mut borrow` 的实参必须是可变且可寻址的 place expression，例如可变局部、可变字段或可变解引用；
临时计算值不能作为可变借用实参。共享借用可以短暂借用临时值，但该借用不能逃出当前完整表达式。
部分应用保存的借用从应用该参数组时开始，并持续到部分应用闭包最后一次使用或析构。

对未标注的泛型参数，传递模式保留为 `auto`，并在单态化时依据实际类型是否实现 `Copy` 决定。
如果 API 需要对所有实例保持相同的消费行为，必须显式写 `copy` 或 `move`。

### 6.2 借用值与生命周期

参数模式会自动建立借用；其他位置可用 `borrow expression` 和 `mut borrow expression` 显式建立
借用值，其类型分别写作 `borrow T` 和 `mut borrow T`：

```sali
let r: borrow i32 = borrow value
let first(borrow values: Slice(i32)): borrow i32 = borrow values[0]
```

函数签名中只有一个输入借用可作为返回来源时，生命周期默认与该输入关联。存在多个可能来源、
借用被保存进结构体、或公开 API 无法唯一推断时，必须显式声明 region 参数：

```sali
let choose('a: region)
  (condition: bool)
  (borrow('a) left: T, borrow('a) right: T): borrow('a) T =
  if condition { borrow left } else { borrow right }
```

region 是编译期参数，不存在于运行时。省略 region 不代表 `'static`；字符串字面量等真正静态数据
由预定义 region `'static` 表示。借用检查首先采用基于最后一次使用的非词法生命周期；诊断必须
指出借用建立点、冲突使用点和借用结束条件。

### 6.3 资源释放与拥有容器

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

### 7.1 块表达式

块的最后一个无分号表达式是块值；空块值为 `()`：

```sali
let n = do {
  let x = 20
  x + 22
}
```

### 7.2 闭包字面量

```sali
let empty = {}
let succ = { (x: i32) ->
  x + 1
}
let thunk = { -> expensive_work() }
let curried = { (x: i32)(y: i32) -> x + y }
```

`{}` 保留为零参数、返回 `()` 的闭包，以满足简洁写法。带参数闭包必须含 `->`。
零参数但非空的闭包写 `{ -> expression }`。闭包可以像命名函数一样声明多个参数组。

为彻底消除 `{ ... }` 究竟是立即执行块还是闭包的歧义，值位置中的立即执行块必须写
`do { ... }`。函数体、`if`、`match` 等语法要求的块不写 `do`。

因此：

```sali
let f = {}             // 类型为 (): ()
let x = do {}          // ()
let f(x: i32) = {}     // 函数体，返回 ()
```

大括号含义完全由语法上下文确定：

| 上下文 | `{ ... }` 的含义 |
|---|---|
| `let f = { ... }` 或普通表达式位置 | 闭包；非空零参闭包需要 `->` |
| `let f(...)= { ... }` 或带名签名声明 RHS | 函数体 |
| `if` / `else` / `while` / `for` / `loop` 后 | 控制流主体 |
| `struct` / `enum` / `trait` / `extend` 后 | 声明体 |
| `value match { ... }` | match 分支列表 |
| `do { ... }` | 立即执行的块表达式 |

match 分支需要执行多条语句时也写 `do { ... }`；写 `{ -> ... }` 明确表示该分支返回一个闭包。
函数需要返回空闭包时可写 `let make() = do { {} }`，避免把 `{}` 解释为空函数体。

### 7.3 捕获模式

普通闭包默认按最小权限自动捕获：只读使用为 `borrow`，修改为 `mut borrow`，消费为 `move`。
即使外部值实现 `Copy`，普通闭包的只读捕获仍优先共享借用，避免是否逃逸反向改变捕获方式。
可以显式指定整个闭包为移动捕获：

```sali
let task = move { -> consume(buffer) }
```

`move` 闭包对 `Copy` 外部值复制，对其他外部值移动。逃逸闭包不得捕获寿命不足的借用。

每个闭包具有匿名具体类型，并依据闭包体如何使用捕获实现一种或多种调用 trait：

- `Fn`：可通过共享借用重复调用；
- `FnMut`：调用可能修改捕获，需要闭包位于可变 place；
- `FnOnce`：调用会消费捕获或闭包自身，最多调用一次。

函数签名 `(T): U` 用于声明和约束调用形状；捕获闭包的大小、析构和调用能力属于其匿名具体类型。
高阶函数应以泛型 callable 接收闭包，避免隐式装箱：

```sali
let apply_twice(T: type)(F: type)
  (value: T)
  (mut borrow function: F): T
where F: FnMut((move _: T): T) = function(function(value))
```

需要异构存储或动态分派时使用未来标准库的显式 `DynFn`/`BoxFn` 类型，不让裸 `(T): U`
悄悄分配堆内存。

## 8. 结构体、构造与成员

### 8.1 名义结构体

```sali
let A = struct(foo: i32, bar: u32)
let a = A(foo: 1, bar: 2)
let b = A(1, 2)
```

结构体是名义类型。构造时允许全标签形式或全位置形式，不允许混用。标签形式不依赖字段顺序，
并且推荐用于公开 API。所有字段都必须初始化且每个字段只能出现一次。

字段默认模块私有、不可通过不可变绑定修改。若结构体值位于可变绑定中，可修改其可见字段：

```sali
let mut a = A(1, 2)
a.foo = 3
```

### 8.2 扩展和关联成员

```sali
extend A {
  let reset(mut borrow self)(): () = {}
  let bar = 42
}

a.reset()
A.bar
```

带 `self` 参数的函数是实例方法；不带 `self` 的声明是关联成员。忽略位于开头的编译期参数组后，
`self` 必须独占第一个运行时参数组，并可使用 `self`、`borrow self`、`mut borrow self`、
`move self`、`copy self`。其类型隐式为扩展目标 `Self`。实例方法必须再声明至少一个显式运行时
参数组；无其他参数时写空组 `()`。这避免 `a.member` 在字段读取和隐式调用之间产生歧义。

方法调用：

```sali
a.reset()
```

脱糖为先应用接收者参数组，再应用源代码中的显式参数组：

```sali
A.reset(a)()
```

调用处不重复写传递模式；编译器依据方法签名对 `a` 建立借用、复制或移动。不允许仅靠重载使
该选择产生歧义。

`Self` 在 `extend` 成员的类型和表达式位置都表示当前扩展目标，因此构造器、关联成员、限定方法
调用和 enum pattern 可分别写成 `Self(value)`、`Self.member`、`Self.method(self: value)()` 与
`Self.Some(value)`。它也适用于泛型扩展、trait 实现和默认 trait 方法；在 `extend` 外使用表达式级
`Self` 是错误。

### 8.3 泛型结构体

```sali
let Box(T: type) = struct(value: T)
let a = Box(i32)(value: 10)
let b = Box(20)
```

`Box(i32)` 是类型，随后一组括号才调用其构造器。`Box(20)` 省略编译期参数组，`T` 由构造实参
推断为 `i32`。类型构造器只在编译期求值，并对实际使用的类型组合单态化。

### 8.4 枚举与封闭和类型

枚举使用与其他类型一致的 `let` 声明：

```sali
let Option(T: type) = enum {
  Some(T),
  None,
}

let Result(T: type, E: type) = enum {
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

```sali
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

`struct(...)` 创建运行时名义数据类型，包括零字段的 `struct()`；`struct { ... }` 创建编译期模块。
两者在首版都只能直接出现在命名 `let` 声明的右侧，不支持匿名名义类型。

实例字段与固有实例方法共享实例成员命名空间，同名时报错，避免 callable 字段与方法调用产生
歧义。关联成员通过 `A.member` 访问，可以与实例字段同名。多个 trait 提供同名方法且上下文无法
唯一选择时，必须使用完全限定调用 `<A as Trait>.method(a)(...)`。模块不是类型，不能构造、实现
trait 或用 `extend` 重新打开。

## 9. 泛型函数与约束

```sali
let identity(T: type)(value: T): T = value
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

```sali
let value: Box(i64) = Box(10)
let made: Product = make(10)
```

类型位置本身没有运行时实参提供证据，因此泛型类型必须写全，例如 `Box(i64)`。`Box(_)`、
`identity(_)(20)`、`identity(T: _)(20)` 和独立表达式 `_` 都是语法错误。数组长度等非类型
编译期参数同样不能写 `_`；需要推断时省略整个编译期参数组，并可用真实的命名参数消歧。

使用 `where` 表达 trait 约束：

```sali
let twice(T: type)(x: T): T
where T: Add(T, Output = T), T: Copy = x + x
```

没有约束的泛型函数只能使用对所有 `T` 都成立的操作。

```sali
let duplicate(T: type)(copy value: T): T
where T: Copy, = {
  let first = value
  value
}
```

## 10. Trait 与实现

```sali
let Foo = trait {
  let f(borrow self)(x: i32): i32
}

extend A: Foo {
  let f(borrow self)(x: i32): i32 = x
}
```

### 10.1 关联类型

```sali
let Bar = trait {
  let Item: type
}

extend A: Bar {
  let Item = i32
}
```

关联类型通过 `T.Item` 或完全限定形式 `<T as Bar>.Item` 引用。存在歧义时必须使用完全限定形式。

关联类型本身也可以接受编译期参数，从而表达容器重新绑定：

```sali
let Chain = trait {
  let Item: type
  let Rebind(U: type): type
}
```

约束中的 `Output = T` 是关联类型等式，不是运行时命名实参：

```sali
where T: Add(T, Output = T)
```

### 10.2 泛型 trait 与泛型实现

trait 自身的类型参数写在名称之后，`Self` 表示实现目标：

```sali
let Convert(To: type) = trait {
  let convert(move self)(): To
}
```

泛型实现先声明该实现引入的编译期参数，再写目标类型和可选 trait：

```sali
extend(T: type) Box(T): Display
where T: Display {
  let display(borrow self)(): String = ...
}
```

```sali
let Cell(T: type) = struct(value: T)

extend(T: type) Cell(T) {
  let new(move value: T): Cell(T) = Cell(value)
  let take(move self)(): T = self.value
}

let cell = Cell.new(42)
let value = cell.take()
```

实现参数必须能从目标类型、trait 参数或 where 约束唯一决定，防止产生无法选择的自由参数。

### 10.3 一致性规则

`Copy`、`Drop`、`Fn`、`FnMut`、`FnOnce`、`Try`、`FromResidual`、运算符协议和 `Future` 是
编译器登记的 lang-item traits，但其声明由匹配工具链版本的 `core` 提供。首版只做静态分派；
trait object 及动态分派留作独立设计，不让 `Foo` 默认同时表示 trait object 类型。

### 10.4 运算符

运算符是 trait 调用的语法糖，例如：

```sali
a + b   // Add.add(a, b)
a == b  // Eq.eq(borrow a, borrow b)
```

运算符优先级和求值顺序由语言固定，trait 只能改变操作含义，不能改变解析方式或短路规则。
`&&`、`||`、赋值、成员访问、普通调用以及 `.await` 不可重载。

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
| `.try` | `Try` + `FromResidual` |

`!=`、`<=` 等可以由核心 trait 的基本结果组合，但每个操作数仍只求值一次。用户不能声明新的
运算符 token 或改变优先级。

## 11. 模块

```sali
let Math = struct {
  let zero = 0
  let inc(x: i32) = x + 1
}

let one = Math.inc(Math.zero)
```

模块在语法上使用无数据的 `struct { declarations }`，但在语义上是编译期命名空间，不是可实例化
的零字段运行时结构体：不能构造、移动、比较或作为普通值传递。这样保留“模块是不带数据的结构体”
的统一成员模型，同时避免为命名空间制造运行时值。

模块成员默认私有。`pub(package)` 对当前包公开，`pub` 同时对依赖该包的代码公开：

```sali
pub let Client = struct(...)
pub(package) let parse_header(text: Str) = ...
let validate_internal_state() = ...
```

私有成员可由声明模块及其子模块访问。公开声明的签名不能泄漏可见性更低的类型或 trait。
模块不能实现运行时 trait，也不能作为普通值捕获。`extend` 只扩展名义数据类型或实现 trait，
不能重新打开模块；一个显式内联模块的成员必须写在其 `struct { ... }` 声明中。

### 11.1 文件模块

每个 `.sali` 文件都是一个隐式模块，模块路径由 `src` 下的相对路径确定：

```text
src/lib.sali       -> 包的库根模块
src/main.sali      -> 默认二进制根模块
src/bin/tool.sali  -> 名为 tool 的额外二进制根模块
src/net.sali       -> net
src/net/http.sali  -> net.http
```

同一路径不能同时由多个源文件定义。文件不需要 `mod` 声明；构建系统发现当前 target 可达的模块。
文件模块与 `let Math = struct { ... }` 声明的内联模块使用相同的成员访问和可见性规则。

### 11.2 导入

`use` 只建立名称别名，不执行文件、复制声明或改变可见性：

```sali
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
path = "src/lib.sali"

[[bin]]
name = "hello-salicin"
path = "src/main.sali"

[dependencies]
local_util = { path = "../local-util" }
```

默认存在 `src/lib.sali` 时生成库 target，存在 `src/main.sali` 时生成同名二进制 target；清单可通过
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

```sali
let main(): i32 = 0
```

其返回类型必须实现标准库的 `Termination` trait。M0 只内建 `()`（退出码 0）和 `i32`；标准库
随后为 `Result((), E)` 提供实现，要求 `E: Display`。不为 `Future(T)` 隐式选择执行器；异步程序
在同步 `main` 中显式调用 `std.async.block_on`。命令行参数和环境通过 `std.env` 显式读取，不进入
平台相关的 `main` ABI。`pub main` 不会自动导出 C 符号；源码入口、模块可见性与链接器导出是
三个独立概念。

## 12. 模式匹配

关键字固定为 `match`（原示例中的 `march` 视为拼写错误）：

```sali
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
- `borrow name`、`mut borrow name`、`move name` 显式绑定模式。

匹配拥有的值默认遵循普通读取规则：`Copy` 字段复制，否则移动；匹配借用值时绑定默认为借用。
守卫只允许观察绑定，不应在分支确定前消费它们。

### 12.1 `match` 的位置与分支规则

Salicin 固定采用后缀 `match`：先写被检查表达式，再写 `match` 和分支。所谓“与 Rust 相同”指
pattern 的解构、穷尽和所有权规则相近，不表示关键字位置相同。被检查表达式只求值一次。

```sali
compute() match {
  Ok(value) if value > 0 => value,
  Ok(_) => 0,
  Err(error) => throw error,
}
```

分支从上到下测试；有守卫的 pattern 即使覆盖某个 variant，也不计入无守卫的穷尽覆盖。`|` 两侧
必须绑定相同名称，并为每个名称产生相同类型和传递模式。不可达分支默认产生编译警告。

## 13. 控制流

### 13.1 条件表达式

条件必须是 `bool`，不提供整数、指针或容器的隐式 truthiness：

```sali
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

```sali
if let Some(value) = option {
  use(value)
}
```

`if let` 不承担穷尽检查，绑定只在 then 块可见；需要处理全部情况时使用 `match`。

在 `if`、`while` 和 `for` 的最外层控制头中禁用尾随闭包，第一个未被括号包围的 `{` 总是
控制流主体。条件本身需要尾随闭包时必须加括号：

```sali
if (validate(input) { (error: Error) -> log(error) }) {
  continue_work()
}
```

### 13.2 循环

```sali
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

`loop` 是表达式，其类型由所有带值 `break` 的值统一；没有可达 `break` 时类型为 `never`。
`while` 和 `for` 的类型固定为 `()`，其中的 `break` 不能携带非 `()` 值。`continue` 开始下一次
迭代。带标签的多层跳转暂不进入核心语法。

`for pattern in expression` 通过标准库 `IntoIterator`/`Iterator` trait 展开，被迭代表达式只求值
一次。pattern 每次迭代重新绑定，其 move/borrow 行为由迭代器的 `Item` 类型决定。

### 13.3 函数退出、赋值与不可恢复失败

`return expression` 立即退出当前命名函数或当前闭包，类型为 `never`。省略表达式等同
`return ()`。函数体最后的表达式是隐式返回值，但不会隐式包装进任意用户类型。

普通赋值及 `+=` 等复合赋值的类型均为 `()`。复合赋值只求值一次左侧 place，并通过对应的
赋值 trait（例如 `AddAssign`）实现；它不是简单的文本改写 `x = x + y`。

可恢复失败使用 `Option`/`Result` 和 `.try`。首版 panic 策略固定为终止进程（abort），用于数组
越界、违反断言等无法在当前 API 中恢复的错误；不进行栈展开，也不允许 panic 穿过 C ABI。

## 14. 可空类型与条件传播

`Option(T)` 是标准库中的封闭和类型：`Some(T) | None`。语言为两种操作提供协议。

### 14.1 可选链 `?.`

```sali
user?.address?.city
result?.normalize()
```

`?.` 对成功分支执行后续成员访问或调用，对空/错误分支保持原容器并跳过后续操作。其协议为
`Chain`，使用泛型关联类型表达“换掉成功值、保留容器形状”：

```sali
let Chain = trait {
  let Item: type
  let Rebind(U: type): type

  let chain(U: type)(F: type)
    (move self)
    (move transform: F): Rebind(U)
  where F: FnOnce((move _: Item): U)
}
```

- `Option(T)?.f` 得到 `Option(U)`；`None` 保持 `None`。
- `Result(T, E)?.f` 得到 `Result(U, E)`；`Err(e)` 保持 `Err(e)`。

编译器把 `value?.member` 的后续操作构造成传给 `Chain.chain` 的闭包。链中操作若自身返回同类容器，
默认不自动展平；需要显式 `flat_map`。标准 `Option` 和 `Result` 的实现消费左侧容器，确保失败
residual 只移动或析构一次；借用容器可由标准库提供单独的 `Chain` 实现。

### 14.2 合并运算符 `??`

```sali
let port = configured_port ?? 8080
let data = read() ?? empty_data
```

`??` 在左侧成功时取出 `T`，否则惰性计算右侧 `T`。它通过 `Coalesce` trait 实现，编译器把右侧
包装为零参数 `FnOnce`，所以右侧严格按需执行。结果类型为 `T`：

```sali
let Coalesce = trait {
  let Item: type
  let coalesce(F: type)
    (move self)
    (move fallback: F): Item
  where F: FnOnce((): Item)
}
```

- `Option(T) ?? T` 的结果为 `T`
- `Result(T, E) ?? T` 的结果为 `T`

若需要使用错误值恢复，调用标准库方法：

```sali
let value = result.recover { (error: Error) -> fallback(error) }
```

`recover` 是普通方法，不是语言语法；它通过独立参数组接收尾随闭包。

`?.` 与 `??` 和其他可重载运算符一样允许用户类型实现，但实现必须满足 `Chain`/`Coalesce`
lang-item trait。短路与“右侧最多求值一次”仍是语言保证，用户实现不能改变。

## 15. 错误处理与 `Try`

```sali
let f(x: i32): Result(i32, Error) = {
  let a = foo().try
  bar(a).try
}
```

`.try` 是后缀传播操作：成功时提取值，失败时把 residual 转换成当前传播边界的返回 residual，
并立即离开该边界。其核心协议为：

```sali
let Try = trait {
  let Output: type
  let Residual: type
  let branch(move self)(): ControlFlow(Residual, Output)
  let from_output(move output: Output): Self
}

let FromResidual(R: type) = trait {
  let from_residual(move residual: R): Self
}

let FromError(E: type) = trait {
  let from_error(move error: E): Self
}
```

概念上，`value.try` 对应：

```sali
value match {
  Continue(x) => x,
  Break(r) => propagate ReturnType.from_residual(r),
}
```

其中 `propagate` 是说明语义使用的伪操作，不是源码关键字。它离开最近的传播边界，而不是无条件
退出外层函数。`Option`、`Result` 和其他控制流容器可以实现 `Try`。

### 15.1 成功值的自动包装

当函数声明的逻辑返回类型 `R` 实现 `Try` 时，函数体按 `R.Output` 检查；正常到达尾表达式后调用
`R.from_output`。`return value` 同样把 `value: R.Output` 包成成功值后退出。这正是下面代码不需要
显式 `Ok(...)` 的原因：

```sali
let load(): Result(Document, Error) = {
  let bytes = read_file().try
  parse(bytes).try
}
```

若手中已有一个 `Result(T, E)` 并希望把其成功值作为当前函数结果，应写 `return result.try`；这会
传播错误并重新使用当前边界的 `from_output` 包装成功值。不会在普通赋值、函数实参或非传播边界
中自动把 `T` 包装成 `Result(T, E)`。

### 15.2 显式传播块

传播边界是最近的 Try 返回函数、闭包或显式 `try do` 块。普通 `do` 块不建立新边界。

`try do` 的容器类型首先从期望类型和其中 residual 推断；无法唯一决定时在 `try` 后标注：

```sali
try Result((), Error) do {
  let a = foo().try
  bar(a).try
} match {
  Ok(_) => println("success"),
  Err(_) => println("failed"),
}
```

正常块尾值由该容器的 `Try.from_output` 包装，失败只离开此块。`.try` 位于闭包内时只传播出该
闭包的最近 Try 边界，不能越过闭包返回外层函数。

`throw err` 是：

```sali
propagate ReturnType.from_error(err)
```

的语义糖；其中 `propagate` 仍是上述伪操作。它要求当前传播边界实现 `FromError(E)`。在
`try do` 内它退出该块，在函数中退出函数；
`throw` 的类型为 `never`。

标准库默认只提供 `Option` 到 `Option`、以及 `Result(T, E1)` 到 `Result(T, E2)` 的 residual 转换，
后者要求 `E2` 可由 `E1` 转换。不会隐式把 `Option` residual 传播成 `Result`。`return` 只离开
当前命名函数或当前闭包；`.try` 和 `throw` 则遵循最近的 Try 传播边界。

## 16. 异步

```sali
let f(x: i32): Future(i32) = {
  let a = foo().await
  a + x
}
```

带源码函数体且显式返回 `Future(T)` 的声明是异步函数；这里的 `Future(T)` 是异步效果标记与
opaque 返回约束，不表示所有异步函数共享同一个装箱类型。每个异步函数生成唯一的匿名状态机类型，
该类型实现 `Future(Output = T)`。局部变量可以持有推断出的具体 Future；高阶 API 以泛型约束接收：

```sali
let run(T: type)(F: type)(move future: F): T
where F: Future(Output = T) = ...
```

需要异构存储或隐藏于非返回位置时，显式使用标准库的 `BoxFuture(T)`。这样普通异步调用不要求堆
分配或动态分派。

完整应用 `f(1)` 会立即求值并按模式转移所有参数，然后返回冷 Future；在首次轮询前不执行源码
函数体。函数体按逻辑输出 `T` 检查，而不是要求最后表达式再构造 Future。因此上例尾表达式是
`i32`。多参数组异步函数只有在最后一组应用后才创建 Future；之前仍是普通部分应用闭包。

`.await` 只能出现在 Future 函数或显式异步上下文中，消费被等待的 Future 并取得其输出；同一个
Future 不能完成两次。异步闭包和立即异步块分别写：

```sali
let task = async { -> fetch().await }
let future = task()                 // 调用异步闭包才创建 Future
let immediate = async do { fetch().await } // 立即创建 Future，不立即执行块体
```

异步函数编译成状态机；跨 `.await` 存活的局部值成为状态机字段。借用外部输入可以跨 `.await`，
相应 region 会成为 Future 类型的一部分，并限制 Future 的逃逸范围。首版拒绝让 Future 借用自己
的其他状态字段并跨挂起点，以避免自引用状态机；底层 poll API 通过 `Pin` 保证已开始轮询的状态机
不再移动。直接递归异步调用形成无限状态机，必须通过 `BoxFuture` 等显式间接层打断。

丢弃尚未完成的 Future 表示取消：按已初始化状态析构所有字段并释放所持借用，不会让任务在后台
继续执行。语言不隐式选择线程池或事件循环；执行器属于 `std.async`，二进制入口若要运行 Future
必须显式调用执行器。

`Future(Result(T, E))` 中，Future 函数体按内层 Try 的 `Output = T` 检查：正常完成先调用
`Result.from_output`，再完成 Future；`.await` 只处理异步层，`.try` 只处理错误层：

```sali
let value = fetch().await.try
```

两者不做隐式合并，保证控制流可读。

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

函数不支持仅按返回类型重载。首版也禁止同一作用域中的同名参数重载，以显式 trait 方法或不同
名称替代，减少与柯里化、默认传递模式之间的组合歧义。

核心隐式 coercion 仅包括 `never` 到任意类型、`mut borrow T` 到 `borrow T`，以及明确登记的
unsizing（例如借用固定数组得到 `borrow Slice(T)`）。整数之间不隐式扩宽，容器不隐式包装，
用户自定义转换也不参与重载搜索。

无损转换使用 `From`/`Into`，可能失败的转换使用 `TryFrom`，明确的位级重解释只能在 `unsafe`
API 中进行：

```sali
let wide = i64.from(value)
let narrow = i32.try_from(wide).try
```

类型名形似调用的语法只调用真实构造器，不自动赋予截断或符号改变语义。

## 19. LLVM 与 ABI 设计边界

这些是实现策略，不属于可观察语义：

- 泛型函数和泛型结构体单态化。
- 不捕获闭包可降低为函数指针；捕获闭包降低为环境结构体加调用函数。
- 柯里化的部分应用使用同一闭包表示。
- trait 静态分派在单态化后直接调用；动态 trait object 若加入则使用显式 witness/vtable。
- `Future` 降低为状态机；`.await` 是状态转换点。
- `Option` 可以利用 niche 优化，但布局只在显式稳定 ABI 属性下成为承诺。

默认 ABI 是语言私有 ABI，可随编译器版本变化。与 C 互操作应使用显式 `extern "C"`，并限制为
C 可表示的签名和布局。

### 19.1 `unsafe` 与 C FFI

内存安全核心之外的操作必须显式进入 `unsafe do` 块或 `unsafe let` 函数。`unsafe` 允许调用者承担
编译器无法证明的前置条件，但不会关闭普通类型检查、借用检查或可见性检查。

```sali
@repr(C)
pub let Point = struct(x: f64, y: f64)

extern "C" {
  @link_name("puts")
  let c_puts(text: Ptr(c_char)): c_int
}

@export_name("salicin_add")
pub extern "C" let add(a: c_int, b: c_int): c_int = a + b
```

`Ptr(T)` 和 `MutPtr(T)` 是不受借用检查器保护、可以为空的原始指针。解引用、指针算术和调用导入的
C 函数需要 `unsafe`。`core.ffi` 提供 `c_char`、`c_int`、`c_long` 等平台 C 类型；Salicin
`char` 是 Unicode scalar，不能代替 C `char`。

```sali
let pointer = unsafe do { raw_alloc(T)(size: bytes, align: alignment) }
unsafe do { raw_dealloc(pointer: pointer, size: bytes, align: alignment) }
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
  `Try`、`Future`/`Poll`、原始指针等；
- `alloc`：依赖可替换分配器，包含 `Box`、`Vec`、`String`、`Rc`、`Arc` 和集合；
- `std`：依赖宿主系统，包含 IO、文件、路径、环境、进程、时间、线程、同步、网络和异步执行器。

prelude 是按 edition 固定的一小组隐式导入，仅容纳普遍需要的语言基础名称。运算符 trait、拥有容器
和宿主 API 属于普通模块，使用 `use core.ops...`、`use alloc...` 或 `use std...` 显式引入。升级标准库
不能向旧 edition 静默加入产生冲突的 prelude 名称。
编译器登记的 lang-item 声明必须来自版本匹配的 `core`。无标准库目标可以只链接 `core` 和最小
运行时；是否启用 `alloc`/`std` 由 target 与项目清单决定。LLVM IR/bitcode 和 Salicin 私有 ABI
都不是稳定的跨编译器版本发布格式。
