# Salicin 语法骨架

状态：演进中的语法参考
源码后缀：`.sc`
源码编码：UTF-8

本文给 lexer 和 parser 提供可实现的语法骨架。语义、类型与所有权规则以
[语言规范](specification.md)为准。这里的 EBNF 尚不是用于标准化的最终 grammar，但每项歧义都必须
在 parser 测试中得到唯一 AST。

## 1. 记号

```text
"token"       固定 token
NAME          lexer 产生的 token 类别
[ x ]         可选一次
{ x }         重复零次或多次
x | y         二选一
( x )         文法分组
```

只有结构性语法词由 lexer 产生固定 token。编译期 domain 名与成员
`type`、`region`、`mut`、`copy`、`move`，借用构造器 `borrow`，以及
`core.control` 提供的 `do`、`try`、`throw`、`unsafe` 都按普通 `IDENT` 词法化。
parser 只在对应语法位置识别其上下文含义，因此这些拼写可以用于其他声明、成员和路径。
已经移除的 `await` 也不是表达式关键字。

## 2. 词法 token

```ebnf
IDENT   = Unicode_XID_Start, { Unicode_XID_Continue } ;
REGION  = "'", Unicode_XID_Start, { Unicode_XID_Continue } ;
INTEGER = decimal_integer | hex_integer | octal_integer | binary_integer ;
FLOAT   = decimal_float ;
CHAR    = "'", char_content, "'" ;
STRING  = '"', { string_content }, '"' ;
```

region token 与字符字面量可由 `'` 后第一个字符区分：`'a` 是 region，`'a'` 是字符。源码在名称
比较前按 NFC 规范化。数值中的 `_` 仅作视觉分隔，不进入数值。

注释：

```ebnf
line_comment  = "//", { any_except_newline } ;
block_comment = "/*", { text | block_comment }, "*/" ;
```

块注释允许嵌套。注释等价为空白，但行注释末尾的物理换行仍参与逻辑换行判定。

## 3. 逻辑换行与语句

lexer 产生 `NEWLINE`，但在以下情况忽略物理换行：

1. 当前位于未闭合的 `(...)` 或 `[...]` 内；
2. 上一个有效 token 是二元/前缀运算符、逗号、`.`、`?.`、`=`、`=>`、`->` 或 `:`。

`{...}` 不抑制换行，因为块内需要分隔语句。后缀 `!` 与 `!!` 可以结束逻辑行；前缀 `!`
后的换行仍继续当前表达式。普通调用不能跨逻辑换行从 `(` 继续：

```sc
f
(x) // 两个表达式，不是 f(x)
```

```ebnf
separator  = NEWLINE | ";" ;
separators = { separator } ;
```

换行分隔表达式但不主动丢弃最后表达式的值。块中最后一个表达式即使后面有换行，只要没有显式
`;`，仍是块值。`;` 明确把该表达式转换为 `()`。

`return`、`throw`、`break` 和 `continue` 后的逻辑换行结束该控制表达式。

## 4. 源文件与声明

```ebnf
source_file = separators, { item, separators }, EOF ;

item = { attribute }, [ visibility ],
       ( use_decl | let_decl | extend_decl | extern_decl ) ;

attribute  = "@", IDENT, [ "(", [ attribute_args ], ")" ] ;
visibility = "pub", [ "(", "package", ")" ] ;
```

### 4.1 `let`

```ebnf
let_decl = "let", [ "mut" ], IDENT,
           { parameter_group },
           [ ":", ( type_expr | "type" | constructor_kind ) ],
           [ with_clause ],
           [ where_clause ],
           [ "=", initializer ] ;

with_clause = IDENT("with"), "(", effect, { ",", effect }, [ "," ], ")" ;
effect = IDENT, [ "(", type_expr, { ",", type_expr }, [ "," ], ")" ] ;

initializer = expression | opaque_type_decl | effect_decl | domain_decl
            | struct_decl | enum_decl | trait_decl ;

opaque_type_decl = "type" ;

effect_decl = "effect", [ "{", separators,
              { effect_operation, separators }, "}" ] ;
effect_operation = "let", IDENT, parameter_group, { parameter_group },
                   ":", type_expr, [ with_clause ] ;

domain_decl = "domain", [ "{", separators,
              { domain_member, separators }, "}" ] ;
domain_member = IDENT | "mut" | "copy" | "move" | "type" | "region" ;

constructor_kind = compile_parameter_group,
                   { compile_parameter_group }, ":", ( "type" | IDENT("effect") ) ;
```

语义限制：

- 普通值、函数和类型声明必须有 initializer；只有 trait 要求，以及编译器内嵌并验证的
  `core.control` 函数契约可以省略。普通包中的无函数体声明是语义错误。
- `let mut` 不能含参数组，且必须绑定运行时值。
- `with(...)` 属于函数签名，位于返回类型之后；`with(Throws(E))` 声明标准可恢复错误 effect，
  `with(Unsafe)` 增加当前内建 `Unsafe` 调用要求。
- `with` 和声明右侧的 `effect` 是上下文词，不是全局关键字。`let UI = effect` 声明名义 marker；
  `let Unsafe = effect {}` 是等价的显式空 operation 形式；`let State(S: type) = effect { ... }`
  还可声明无函数体的 operation requirements。这些声明向 `effect` domain 引入成员。旧的
  `(effect): T`、`T(effect)` 与 `T ! effect` 都不属于语法。
- 声明右侧的 `domain` 同样是上下文词，用于声明编译期参数域。无 body 的 `domain` 是开放域；
  `domain { ... }` 是封闭域。标准 `type`、`region`、`effect`、`parameters`、`access` 与 `passing` domain 位于
  `core.domains`；effect 身份位于 `core.effect`；控制 lang item 可在声明名位置使用 `do`、`try`、
  `unsafe`、`loop`。
- 声明右侧的 `type` 声明新的不透明名义类型，例如 `pub let i32 = type`。可选的封闭值集合
  声明编译器表示的全部合法值，例如 `pub let bool = type { false, true }`。它不同于
  `let Alias: type = Target` 透明别名；只有经过验证的 core primitive lang item 才获得编译器原生布局。
- `let f(x: T) = { body }` 是把参数提升到名称旁边的具名闭包声明；RHS 必须有花括号。
- `let f: (x: T): R = { body }` 是带名签名的具名闭包声明：所有槽必须有名字。
- `let f: (T): R = { (x: T) -> body }` 是普通函数值绑定。
- `let Alias(T: type): type = Target(T)` 定义透明类型族；`let Constructor:
  (T: type): type = Target` 直接绑定类型构造子。前者的 RHS 必须是已应用的具体类型，不进行隐式
  eta 应用。
- 编译期参数 kind 可以写成构造子签名，例如 `F: (Value: type): type` 与
  `E: (Error: type): effect`。`let TraitName(...) = trait(Self: Kind)` 中名称旁参数是真正的
  trait 参数；`trait(Self: Kind)` 声明被实现主体的 kind，省略时为 `Self: type`。匹配 arity 的
  泛型 nominal 构造子可以实现 `Self` 为构造子 kind 的 trait；method implementation 会注册为
  generic function template 并接受模板验证。receiver-style constructor trait 方法可以从具体
  nominal 实例分派，例如 `Carrier(i32) { value: 41 }.map(add_one)`；无 receiver 的 constructor trait
  associated function 可以通过裸构造子调用。关联类型 lowering 与完整 HKT 方程求解仍会被显式
  拒绝或留待后续。
- 具名函数的参数类型必须显式；首版 `let` 名称位置不接受解构 pattern。
- trait 声明体中的无 initializer `let` 是 requirement。

参数组：

```ebnf
parameter_group = "(", [ parameter_list | parameter_expansion ], ")" ;
parameter_list  = parameter, { ",", parameter }, [ "," ] ;
parameter_expansion = "...", [ pass_mode ], IDENT, ":", type_expr ;

parameter = [ pass_mode | IDENT ], IDENT, ":", type_expr ;

pass_mode = "copy" | "move" ;

access_or_region = IDENT | "shared" | "mut" | REGION ;
```

参数模式位置的 `IDENT` 只有在它引用当前函数已声明的 `P: passing` 参数时才合法；否则第一个
`IDENT` 就是参数名。这是上下文语法，不把 `passing`、`auto` 或参数名加入全局保留字集合。
`(...move args: P)` 把编译期 `parameters` schema 展开为完整的一个运行时参数组；首版要求该
展开独占参数组。关联 `parameters` declaration 可由编译器派生，不能作为普通运行时类型使用。

一个编译期参数组只含 `T: type`、`A: access`、`R: region` 等编译期参数，并位于所有运行时参数组之前；同一组
不能混合编译期和运行时参数。忽略开头的编译期组后，实例方法的 `self` 独占第一个运行时组，
并且后面至少还有一个显式运行时组。

### 4.2 数据与 trait

```ebnf
struct_decl = "struct", [ "(", struct_option_list, ")" ],
              "{", [ field_list ], "}" ;

struct_option_list = struct_option, { ",", struct_option }, [ "," ] ;
struct_option      = "derive", ":", IDENT ;

field_list = field_decl, { ",", field_decl }, [ "," ] ;
field_decl = [ visibility ], IDENT, ":", type_expr ;

enum_decl = "enum", "{", separators,
            [ variant, { ",", separators, variant }, [ "," ] ],
            separators, "}" ;

variant = IDENT
        | IDENT, "(", positional_variant_fields, ")"
        | IDENT, "(", named_variant_fields, ")" ;

positional_variant_fields = type_expr, { ",", type_expr }, [ "," ] ;
named_variant_fields      = field_decl, { ",", field_decl }, [ "," ] ;

trait_decl = "trait", "{", separators, { trait_item, separators }, "}" ;
trait_item = { attribute }, [ visibility ], let_decl ;
```

一个 variant 的字段全部按位置或全部命名；不能混合。`struct { ... }` 在字段上下文建立运行时
名义类型，在声明上下文建立编译期模块；二者都只允许作为命名 `let` initializer。当前实现支持
`struct(derive: Copy) { ... }`，并把它降低为普通 `Copy` trait 实现。

struct 字段与命名 variant 字段默认私有，并可写 `pub(package)` / `pub`。字段有效可见性不会宽于
外层类型；位置 variant payload 没有独立 visibility 语法，继承 enum 声明的可见性。

### 4.3 `extend`

```ebnf
extend_decl = "extend", [ compile_parameter_group ], type_expr,
              [ ":", trait_ref ], [ where_clause ],
              "{", separators, { let_decl, separators }, "}" ;

compile_parameter_group = "(", compile_parameter,
                          { ",", compile_parameter }, [ "," ], ")" ;
compile_parameter = IDENT, ":", ( "type" | "access" | "passing" | "effect" | constructor_kind )
                  | REGION, ":", "region" ;
```

编译期参数组的语义限制与函数相同。例如：

```sc
extend(T: type) Box(T): Display
where T: Display {
  let display(self: borrow(Self))(): String = { ... }
}
```

### 4.4 导入与 FFI

```ebnf
use_decl = "use", use_path,
           [ ".", "{", use_name, { ",", use_name }, [ "," ], "}" ] ;
use_name = IDENT, [ "as", IDENT ] ;
use_path = path, [ "as", IDENT ] ;

extern_decl = "extern", STRING,
              ( "{", separators, { extern_function_decl, separators }, "}"
              | let_decl ) ;

extern_function_decl = { attribute }, "let", IDENT, parameter_group,
                       ":", type_expr ;
```

`use` 的实际 parser 可把单名、分组和 `as` 形式拆成不同 AST 节点。首版没有 glob 导入。

## 5. 类型

函数类型的 `:` 右结合。函数类型可有多个参数组以及一个位于结果类型之后的 `with(...)` 子句；该子句
不是运行时参数组，也不增加一层柯里化。解析器看到 parenthesized type group 后，仅当后续最终接
`:` 时才把它解释为 callable group；否则它是括号类型或元组类型。

```ebnf
type_expr = callable_group, { callable_group }, ":", type_expr, [ with_clause ]
          | type_atom ;

callable_group = "(", [ signature_slot, { ",", signature_slot }, [ "," ] ], ")" ;

signature_slot = type_expr
               | IDENT, ":", type_expr ;

type_atom = path, [ type_arguments ]
          | "(", ")"
          | "(", type_expr, ")"
          | "(", type_expr, ",", [ type_expr, { ",", type_expr }, [ "," ] ], ")"
          | "borrow", { "(", access_or_region, ")" }, "(", type_expr, ")" ;

type_arguments = "(", type_argument, { ",", type_argument }, [ "," ], ")" ;
type_argument  = [ IDENT, ":" ], type_expr | INTEGER ;
```

`_` 不是类型实参。调用中的编译期参数组可整体省略，并由运行时实参和期望类型推断；显式消歧使用
普通的 `IDENT ":" expression` 命名实参，不增加另一套括号或关键字。
类型位置的构造子实参同样可以写 `IDENT ":" type_expr` 标签；一个实参组不能混用具名和位置形式。
`access` 是 `core.domains` 声明的封闭编译期 domain；其内建实参为 `shared` 与 `mut`。`borrow(A)(T)` 和
`borrow(A)(R)(T)` 分别携带 access 参数以及 access/region 参数组合。
`passing` 是函数编译期 domain；其内建实参为 `auto`、`copy` 与 `move`，并在参数模式位置以
已声明的参数名引用，例如 `(P value: T)`。
`effect` 是函数编译期 domain；实参是完整 effect row：`pure`、`Unsafe`、名义 marker 或其组合。
默认值为 `pure`。参数名只可出现在函数签名的 `with(...)` 子句和其他 effect 编译期实参位置，
例如 `with(E)` 与 `forward(E)(value)`；它也可由 callable 实参或期望类型推断。

无结果类型只写作 `()`；`void` 拼法已删除。`Never` 按普通 prelude 名称解析，等价于
`let Never = enum {}`，不是 lexer 关键字。零 variant enum 合法；其值位置可以用空的
`match {}` 消除。

匿名签名槽只有在模式为 `auto` 时可省略 `_:`：

```sc
(T): U                 // auto 参数，类型 T
(_: borrow(T)): U      // 传入或自动借用一个共享借用值
(_: borrow(mut)(T)): U // 传入或自动借用一个可变借用值
```

trait 引用和约束：

```ebnf
where_clause = "where", predicate, { ",", predicate }, [ "," ] ;
predicate    = type_expr, ":", trait_ref ;

trait_ref = path, [ "(", trait_argument,
                    { ",", trait_argument }, [ "," ], ")" ] ;
trait_argument = type_expr | IDENT, "=", type_expr ;
```

## 6. 表达式与优先级

从低到高：

| 层级 | 构造 | 结合性 |
|---|---|---|
| 1 | `=`、`+=`、`-=`、`*=`、`/=` 等赋值 | 右结合 |
| 2 | 后缀 `match` | 不结合 |
| 3 | `??` | 右结合 |
| 4 | `||` | 左结合 |
| 5 | `&&` | 左结合 |
| 6 | `\|` | 左结合 |
| 7 | `^` | 左结合 |
| 8 | `&` | 左结合 |
| 9 | `==`、`!=` | 不结合 |
| 10 | `<`、`<=`、`>`、`>=` | 不结合 |
| 11 | `<<`、`>>` | 左结合 |
| 12 | `+`、`-` | 左结合 |
| 13 | `*`、`/`、`%` | 左结合 |
| 14 | `-`、`!`、`borrow`、`borrow(mut)`、`move` | 前缀 |
| 15 | 调用、索引、成员、`?.`、后缀 `!` / `!!`、尾随闭包 | 左到右后缀 |

```ebnf
expression       = assignment_expr ;
assignment_expr  = match_expr, [ assign_op, assignment_expr ] ;
assign_op        = "=" | "+=" | "-=" | "*=" | "/=" | "%="
                 | "&=" | "|=" | "^=" | "<<=" | ">>=" ;
match_expr       = coalesce_expr, [ "match", match_body ] ;
coalesce_expr    = logical_or_expr, [ "??", coalesce_expr ] ;
logical_or_expr  = logical_and_expr, { "||", logical_and_expr } ;
logical_and_expr = bitwise_or_expr, { "&&", bitwise_or_expr } ;
bitwise_or_expr  = bitwise_xor_expr, { "|", bitwise_xor_expr } ;
bitwise_xor_expr = bitwise_and_expr, { "^", bitwise_and_expr } ;
bitwise_and_expr = equality_expr, { "&", equality_expr } ;
equality_expr    = relation_expr, [ equality_op, relation_expr ] ;
relation_expr    = shift_expr, [ relation_op, shift_expr ] ;
shift_expr       = additive_expr, { ( "<<" | ">>" ), additive_expr } ;
additive_expr    = multiply_expr, { additive_op, multiply_expr } ;
multiply_expr    = unary_expr, { multiply_op, unary_expr } ;
unary_expr       = { prefix_op }, postfix_expr ;
```

后缀层：

```ebnf
postfix_expr = primary_expr, { postfix_part } ;

postfix_part = call_group
             | "[", expression, "]"
             | ".", IDENT
             | "?.", IDENT
             | "!", [ "!" ]
             | trailing_closure
             | named_trailing_closure ;

call_group = "(", [ call_argument, { ",", call_argument }, [ "," ] ], ")" ;
call_argument = expression | IDENT, ":", expression ;

trailing_closure = closure_literal ;
named_trailing_closure = IDENT, ":", closure_literal ;
```

语义限制：尾随闭包跟随已有 `call_group`，每个尾随闭包新建一个单元素参数组；可以跨行连续提供
多个位置或具名尾随闭包。`f(x) {} {}` 是 `Call(Call(Call(f,[x]),[{}]),[{}])`，
`f(x) label: {}` 的最后一组则包含标签为 `label` 的闭包参数。普通名称后的 `{}` 仍优先解释为
结构体字面量，因此无显式调用组的通用调用写作 `f() {}`；经过验证的控制形式可以提供更短写法，
例如 `while { condition } { body }`。

同一作用域中的具名函数可以形成重载集，但每个候选的运行时参数标签组必须不同。调用重载名时，
至少一个实参必须写成 `label: expression`，所有已提供参数组共同筛选唯一候选；其他参数组仍可使用
位置实参。类型、返回类型、传递模式和 effect 不属于重载身份。inherent 方法也适用，但隐式
`self` 接收者组不算具名消歧证据。trait requirement 及其实现使用相同的标签形状身份，调用时
也按同一规则选择。泛型函数先由运行时具名参数选择模板，再对该模板推断或读取编译期参数组；
编译期参数的名称本身不构成重载证据。

primary：

```ebnf
primary_expr = literal
             | path
             | "(", expression, ")"
             | tuple_literal
             | array_literal
             | do_block
             | try_block
             | unsafe_block
             | closure_literal
             | if_expr
             | loop_expr
             | while_expr
             | for_expr
             | async_expr
             | return_expr
             | throw_expr
             | break_expr
             | continue_expr ;

do_block = "do", block ;
try_block = "try", block ;
unsafe_block = "unsafe", block ;

closure_literal = [ "move" ], "{",
                  [ parameter_group, { parameter_group }, "->" ],
                  block_contents, "}" ;

async_expr = "async", closure_literal ;
```

`raw_alloc(T)(size, align)` 与 `raw_dealloc(pointer, size, align)` 使用普通调用语法，但它们是 edition
保留的 allocator intrinsic，只能出现在 `unsafe` 的动态作用域内；`raw_alloc` 的类型组可由期望
`MutPtr(T)` 省略。

`raw_init(pointer, value)` 是第三个 `unsafe` allocator intrinsic：它以 move 语义把 `value` 初始化到
尚未初始化的 `MutPtr(T)` storage，区别于表示覆盖写且限于 `Copy` pointee 的 `*pointer = value`。

`raw_take(pointer)` 从 `MutPtr(T)` storage move 出 `T`，并将该 storage 留为未初始化；调用者必须在
再次读取或释放 owner 前重新初始化，或只释放 allocation。`forget(value)` 则消费一个 owning value
而不运行 drop glue；它不需要 `unsafe`，但会有意泄漏该值拥有的资源。

`size_of(T)` 与 `align_of(T)` 同样使用普通单组调用外形，但该组只接受一个类型实参；结果为 `u64`，
布局由最终 LLVM target 决定。

每个花括号表达式都是闭包。`{}` 是零参空闭包，`{ expression }` 是非空零参闭包，
`{ (x: T)(y: U) -> expression }` 是带多组参数的闭包。只有显式参数前缀需要 `->`；
已经删除 `{ -> expression }` 拼法。以括号表达式开头的零参闭包（例如 `{ (40 + 2) }`）通过
闭合参数组后是否紧跟 `->` 消歧。

## 7. 块和控制流

```ebnf
block = "{", block_contents, "}" ;

block_contents = separators,
                 { block_item, separators },
                 [ expression, separators ] ;

block_item = let_decl | expression ;
```

实际 parser 应保留每个表达式后的终止符类别；最后一个未以 `;` 终止的表达式成为块值。

```ebnf
if_expr = "if", ( expression | "let", pattern, "=", expression ), block,
          [ "else", ( block | if_expr ) ] ;

loop_expr  = "loop", block ;
while_expr = "while",
             ( closure_literal, closure_literal
             | "condition", ":", closure_literal,
               "body", ":", closure_literal
             | "let", pattern, "=", expression, block ) ;
for_expr   = "for", pattern, "in", expression, block ;

return_expr   = "return", [ expression ] ;
throw_expr    = "throw", expression ;
break_expr    = "break", [ expression ] ;
continue_expr = "continue" ;

```

`for` 的 pattern 必须不可失败；当前实现接受单一名称绑定和 `_`。它通过 `core.iter` 中经验证的
`IntoIterator` 与 `Iterator` lang item 展开，而不是按普通成员查找选择同名方法。

`do { ... }`、`try { ... }`、`unsafe { ... }` 和未来的 `async { ... }` 在语义上都是接受尾闭包的内建函数调用，
不是三种互不相关的块节点。parser 保留专用产生式以消除关键字后大括号的歧义。`do` 立即调用闭包
并原样转发其 effect/color；`try` 把 `Throws(E)` 处理为 `Result(E)(T)`；`unsafe` 处理闭包要求的
`Unsafe` effect。

解析 `if`、`for` 和 `while let` 控制头的最外层时禁用尾随闭包；第一个未被括号包围的 `{`
是控制主体。普通 `while` 则要求条件和主体各使用一个尾随闭包。

### 7.1 大括号上下文

| 上下文 | AST |
|---|---|
| `let f = {}` | 零参闭包 |
| `let f(x: T) = { ... }` | 参数提升后的具名闭包体 |
| `if`/`else`/`loop`/`for` 后 | 控制构造消费的主体闭包 |
| `while` 后 | 依次提供条件与主体的两个零参闭包 |
| `struct`/`enum`/`trait`/`extend` 后 | 声明体 |
| `value match { ... }` | match arm 列表 |
| 其他表达式位置 | 闭包 |
| `do { ... }` | effect 多态的立即尾闭包调用 |

## 8. `match` 与 pattern

```ebnf
match_body = "{", separators,
             [ match_arm, { ",", separators, match_arm }, [ "," ] ],
             separators, "}" ;

match_arm = pattern, [ "if", expression ], "=>", expression ;

pattern = or_pattern ;
or_pattern = bind_pattern, { "|", bind_pattern } ;

bind_pattern = [ pattern_mode ], IDENT
             | "_"
             | literal_pattern
             | tuple_pattern
             | variant_pattern
             | range_pattern ;
pattern_mode = pass_mode | "borrow", [ "(", "mut", ")" ] ;

tuple_pattern   = "(", pattern, ",", [ pattern, { ",", pattern }, [ "," ] ], ")" ;
variant_pattern = path, [ "(", pattern_fields, ")" ] ;
pattern_fields  = pattern, { ",", pattern }, [ "," ]
                | named_pattern, { ",", named_pattern }, [ "," ] ;
named_pattern   = IDENT, ":", pattern ;
range_pattern   = literal_pattern, ( ".." | "..=" ), literal_pattern ;
```

同一 variant pattern 内不混合位置和命名字段。裸大写名称若解析为当前枚举可见的无数据 variant，
表示 variant；其他裸标识符建立绑定。有歧义时使用限定路径，例如 `Option.Some(x)`。或 pattern
两侧绑定集合、类型和模式必须一致。match arm 之间必须有逗号，最后一个逗号可省略。

## 9. 路径

```ebnf
path = path_head, { ".", IDENT } ;
path_head = IDENT | "root" | "self" | "super" | "Self" ;
```

完全限定 trait 成员 `<T as Trait>.member` 作为独立的 `qualified_path` 产生，加入 `path` 可出现的
位置。类型应用 `A { value: T }` 与运行时调用 `f(x)` 具有相同 token 外形，由名称的 kind 和上下文在语义
分析阶段区分。表达式或 pattern 路径头 `Self` 只在 `extend` 成员内有效，并由语义分析替换为当前
具体或泛型扩展目标。

## 10. 必须锁定的 parser 测试

```sc
let f = {}                         // 零参闭包
let f(x: i32) = {}                 // 返回 () 的具名闭包
let x = do {}                      // 单元值
let add = { (x: i32)(y: i32) -> x + y }

f(x) {}                            // 新参数组
f(x, {})                           // 同组第二实参
if f(x) {}                         // 条件 f(x) + 空 if 主体
if (f(x) {}) {}                    // 条件中使用尾随闭包
a.method()                         // A.method(a)()

let by_borrow: (_: borrow(T)): U = callback
let takes_ref: (_: borrow(T)): U = callback_ref

a ?? b match {
  Some(x) => x,
  None => fallback,
}
```

上述每一行都应有 AST snapshot；相邻案例不得产生相同 AST。错误恢复至少覆盖缺失 `)`、缺失
`=>`、enum/match 中缺逗号、控制头意外尾随闭包和闭包缺 `->`。
