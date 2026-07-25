# Salicin Grammar

Status: evolving parser reference

This document describes Salicin's concrete syntax. The
[language specification](specification.md) defines semantics. The notation is EBNF-like:

```text
"token"     literal token
TOKEN       lexical token class
[ x ]       optional
{ x }       zero or more
x | y       alternative
( x )       grouping
```

Contextual words are represented as `IDENT` by the reference lexer and interpreted by the parser
only in the corresponding position. This set includes compile-time kinds, passing modes, borrow
forms, and control-operation names.

## 1. Lexical Grammar

```ebnf
IDENT   = Unicode_XID_Start, { Unicode_XID_Continue } ;
REGION  = "'", Unicode_XID_Start, { Unicode_XID_Continue } ;
INTEGER = decimal_integer | hex_integer | octal_integer | binary_integer ;
FLOAT   = decimal_float ;
CHAR    = "'", char_content, "'" ;
STRING  = '"', { string_content }, '"' ;

line_comment  = "//", { any_except_newline } ;
block_comment = "/*", { text | block_comment }, "*/" ;
```

Names are normalized to NFC. `_` may separate digits inside a numeric literal. A `REGION` and a
character literal are distinguished by the closing quote.

The lexer emits `NEWLINE` except:

- inside unmatched `(...)` or `[...]`;
- after a token that necessarily continues an expression, including an infix or prefix operator,
  comma, `.`, `?.`, `=`, `=>`, `->`, or `:`.

Braces do not suppress newlines.

```ebnf
separator  = NEWLINE | ";" ;
separators = { separator } ;
```

## 2. Source Files and Items

```ebnf
source_file = separators, { item, separators }, EOF ;

item = [ visibility ], ( let_decl | extend_decl )
     | test_registration ;

visibility = "pub", [ "(", "package", ")" ] ;

test_registration =
    contextual("test"), "(", STRING, ")", block ;
```

A test registration cannot have an attribute or visibility. Its string must be
non-empty, and the trailing block is the test body. `test` remains an ordinary
identifier outside this top-level form.

### 2.1 Let Declarations

```ebnf
let_decl = "let", [ contextual("mut") ], IDENT,
           { declaration_group },
           [ ":", declaration_annotation ],
           [ with_clause ],
           [ where_clause ],
           [ "=", initializer ] ;

declaration_group = compile_parameter_group | runtime_parameter_group ;

declaration_annotation =
    type_expr
  | contextual("type")
  | contextual("domain")
  | constructor_kind ;

initializer =
    builtin_initializer
  | foreign_initializer
  | expression
  | effect_decl
  | domain_decl
  | struct_decl
  | enum_decl
  | trait_decl ;

builtin_initializer =
    contextual("builtin"), "(", ")" ;

foreign_initializer =
    contextual("foreign"), "(",
    contextual("c"), [ ",", STRING ],
    ")" ;
```

`let Name: type` declares an opaque nominal type. `let Name: domain` declares an abstract domain.
`let Name = domain { ... }` declares a domain with a known member set. Bare `= domain`, `= type`,
and `= type { ... }` are not productions.

`builtin()` is a complete initializer available only to the embedded `core`
package. It may define a compiler-owned function, type, type constructor, or
extension method whose exact declaration is validated by the edition
contract. It is not an expression initializer available to user packages.

### 2.2 Compile-Time Parameters

```ebnf
compile_parameter_group =
    "(", compile_parameter, { ",", compile_parameter }, [ "," ], ")" ;

compile_parameter =
    [ "..." ], compile_parameter_name, ":", compile_parameter_kind,
    [ "=", compile_parameter_default ] ;

compile_parameter_name = IDENT | REGION ;

compile_parameter_kind =
    contextual("type")
  | contextual("usize")
  | contextual("region")
  | contextual("effect")
  | contextual("parameters")
  | IDENT
  | constructor_kind ;

constructor_kind =
    "(", constructor_kind_parameter,
    { ",", constructor_kind_parameter }, [ "," ], ")",
    ":", contextual("type") ;

constructor_kind_parameter =
    IDENT, ":", ( contextual("type") | contextual("parameters") ) ;
```

Whether a parenthesized declaration group is compile-time or runtime is determined by its
parameter forms. The two classes cannot be mixed in one group.

### 2.3 Runtime Parameters

```ebnf
runtime_parameter_group =
    "(", [ runtime_parameter, { ",", runtime_parameter }, [ "," ] ], ")" ;

runtime_parameter =
    { parameter_modifier },
    [ IDENT ],
    ( IDENT | "_" ),
    ":",
    type_expr ;

parameter_modifier =
    contextual("copy")
  | contextual("move")
  | contextual("mut")
  | contextual("shared")
  | IDENT ;
```

The optional first `IDENT` is an external argument label when followed by a second parameter name.
Parameter modifiers are resolved against the source-backed passing declarations.

### 2.4 Data, Effects, and Traits

```ebnf
domain_decl =
    contextual("domain"), "{", separators,
    { domain_member, separators }, "}" ;

domain_member = IDENT | contextual_word ;

effect_decl =
    contextual("effect"), "{", separators,
    { effect_operation, separators }, "}" ;

effect_operation =
    "let", IDENT, runtime_parameter_group,
    { runtime_parameter_group },
    ":", type_expr, [ with_clause ] ;

struct_decl =
    "struct", [ struct_options ], "{", separators,
    { [ visibility ], IDENT, ":", type_expr, [ "," ], separators },
    "}" ;

struct_options =
    "(", struct_option, { ",", struct_option }, [ "," ], ")" ;

struct_option =
    contextual("c")
  | contextual("derive"), ":", IDENT ;

enum_decl =
    "enum", "{", separators,
    { enum_variant, [ "," ], separators },
    "}" ;

enum_variant =
    IDENT,
    [ "(", [ type_expr, { ",", type_expr }, [ "," ] ], ")"
    | "{", [ named_field, { ",", named_field }, [ "," ] ], "}" ] ;

named_field = [ visibility ], IDENT, ":", type_expr ;

trait_decl =
    "trait",
    [ "(", self_parameter, ")" ],
    { compile_parameter_group },
    [ where_clause ],
    "{", separators,
    { trait_member, separators },
    "}" ;

self_parameter = contextual("Self"), ":", compile_parameter_kind ;

trait_member =
    "let", IDENT,
    { declaration_group },
    ":", ( type_expr | contextual("type") | contextual("parameters") ),
    [ with_clause ],
    [ where_clause ],
    [ "=", expression ] ;
```

An associated type or associated constructor has no runtime parameter groups. Its compile-time
groups appear before `: type`.

### 2.5 Extensions and Predicates

```ebnf
extend_decl =
    "extend", { compile_parameter_group },
    type_expr,
    [ ":", trait_ref ],
    [ where_clause ],
    "{", separators,
    { extend_member, separators },
    "}" ;

extend_member =
    "let", IDENT,
    { declaration_group },
    [ ":", type_expr ],
    [ with_clause ],
    [ where_clause ],
    [ "=", expression ] ;

where_clause =
    "where", where_predicate,
    { ",", where_predicate },
    [ "," ] ;

where_predicate = type_expr, ":", trait_ref ;

trait_ref =
    path,
    [ "(", [ trait_argument, { ",", trait_argument }, [ "," ] ], ")" ] ;

trait_argument = [ IDENT, ":" ], type_expr | associated_binding ;

associated_binding =
    IDENT,
    { compile_parameter_group },
    "=",
    type_expr ;
```

Positional trait arguments precede associated bindings. A generic associated constructor equation
declares its local binders on the left, for example
`Item(R: region) = borrow(R)(T)`.

### 2.6 Foreign Declarations

```ebnf
foreign_function =
    "let", IDENT,
    runtime_parameter_group,
    ":", type_expr,
    "=", foreign_initializer ;
```

A foreign declaration has exactly one runtime parameter group, no
compile-time parameters, explicit effects, `where` clause, or body. Omitting
the string uses the Salicin declaration name as the linker symbol. The only
accepted ABI name is the contextual identifier `c`. Grouped `extern`
declarations and `@` attributes are not grammar productions.

### 2.7 Compiler Definitions

```ebnf
builtin_definition =
    "let", IDENT,
    { declaration_group },
    ":", declaration_annotation,
    "=", builtin_initializer ;
```

The core-private bootstrap has the exact shape
`let builtin(): Never = builtin()`. Every other marker must match a known
compiler-owned edition contract and is removed before code generation.
Trait requirements, effect operations, and user opaque types remain
bodyless declarations rather than builtin definitions.

## 3. Types

```ebnf
type_expr = function_type | postfix_type ;

function_type =
    function_type_group,
    { function_type_group },
    ":", type_expr,
    [ with_clause ] ;

function_type_group =
    "(", [ function_type_parameter,
    { ",", function_type_parameter }, [ "," ] ], ")" ;

function_type_parameter =
    { parameter_modifier }, [ IDENT, ":" ], type_expr ;

postfix_type =
    primary_type,
    { type_argument_group } ;

primary_type =
    path
  | primitive_type
  | tuple_type
  | borrow_type
  | array_type ;

tuple_type =
    "(", type_expr, ",",
    [ type_expr, { ",", type_expr }, [ "," ] ],
    ")" ;

borrow_type =
    contextual("borrow"),
    [ type_argument_group ],
    [ type_argument_group ],
    type_argument_group ;

array_type =
    "[", type_expr, ";", INTEGER, "]" ;

type_argument_group =
    "(", [ type_argument, { ",", type_argument }, [ "," ] ], ")" ;

type_argument = [ IDENT, ":" ], type_expr ;

with_clause =
    contextual("with"), "(",
    effect_ref, { ",", effect_ref }, [ "," ],
    ")" ;

effect_ref = path, [ type_argument_group ] ;
```

`()` is unit, while `(T,)` is a one-element tuple. Curried constructor applications retain each
argument group in the AST.

## 4. Expressions

Precedence is listed from lowest to highest:

```ebnf
expression         = assignment ;
assignment         = propagation, [ assignment_op, assignment ] ;
propagation        = coalescing, { "!", [ "!" ] } ;
coalescing         = logical_or, { "??", logical_or } ;
logical_or         = logical_and, { "||", logical_and } ;
logical_and        = comparison, { "&&", comparison } ;
comparison         = bit_or, [ comparison_op, bit_or ] ;
bit_or             = bit_xor, { "|", bit_xor } ;
bit_xor            = bit_and, { "^", bit_and } ;
bit_and            = shift, { "&", shift } ;
shift              = additive, { shift_op, additive } ;
additive           = multiplicative, { additive_op, multiplicative } ;
multiplicative     = prefix, { multiplicative_op, prefix } ;
prefix             = { prefix_op }, postfix ;
postfix            = primary, { postfix_suffix } ;

assignment_op =
    "=" | "+=" | "-=" | "*=" | "/=" | "%="
  | "&=" | "|=" | "^=" | "<<=" | ">>=" ;

comparison_op = "==" | "!=" | "<" | "<=" | ">" | ">=" ;
shift_op = "<<" | ">>" ;
additive_op = "+" | "-" ;
multiplicative_op = "*" | "/" | "%" ;
prefix_op = "-" | "!" | contextual("move") | contextual("borrow") ;
```

```ebnf
postfix_suffix =
    argument_group
  | bare_argument
  | ".", IDENT
  | "?.", IDENT
  | "[", expression, "]"
  | trailing_closure ;

argument_group =
    "(", [ argument, { ",", argument }, [ "," ] ], ")" ;

argument = [ IDENT, ":" ], expression ;

bare_argument = primary, { argument_group | ".", IDENT | "?.", IDENT | "[", expression, "]" } ;

trailing_closure =
    [ IDENT ], block ;
```

`f value` supplies one positional argument as the next call group. The next
runtime group must therefore contain exactly one parameter. Repeated bare
arguments preserve currying: `f left right` is `f(left)(right)`, not
`f(left, right)`. Bare application binds more tightly than infix operators, so
`f x + y` is `(f x) + y`; use `f (x + y)` to pass the complete infix
expression. A logical newline does not begin a bare argument.

```ebnf
primary =
    literal
  | path
  | tuple_expression
  | array_expression
  | struct_expression
  | block
  | closure_expression
  | match_expression ;

literal = INTEGER | FLOAT | CHAR | STRING
        | contextual("true") | contextual("false") | "()" ;

tuple_expression =
    "(", expression, ",",
    [ expression, { ",", expression }, [ "," ] ],
    ")" ;

array_expression =
    "[", [ expression, { ",", expression }, [ "," ] ], "]" ;

struct_expression =
    type_expr, "{",
    [ field_initializer, { ",", field_initializer }, [ "," ] ],
    "}" ;

field_initializer = IDENT, ":", expression ;

closure_expression =
    [ closure_parameters, "->" ], block ;

closure_parameters =
    IDENT
  | "(", [ runtime_parameter, { ",", runtime_parameter }, [ "," ] ], ")" ;
```

Control operations such as `if`, `while`, `for`, `loop`, `return`, `break`, `continue`, `do`,
`try`, `throw`, and `unsafe` begin as contextual identifiers and are recognized by their validated
call or trailing-closure shape.

## 5. Blocks and Matches

```ebnf
block =
    "{", block_contents, "}" ;

block_contents =
    separators,
    { block_item, separators },
    [ expression, [ NEWLINE ] ] ;

block_item =
    let_decl
  | expression ;

match_expression =
    contextual("match"), expression,
    match_case, { match_case } ;

match_case =
    "{", separators,
    pattern,
    [ contextual("if"), expression ],
    "->",
    block_contents,
    "}" ;
```

`c` selects the C data representation and may appear at most once. It is
orthogonal to named options such as `derive: Copy`; for example,
`struct(c, derive: Copy) { ... }`. Empty option lists retain the ordinary
Salicin representation.

```ebnf
pattern =
    "_"
  | IDENT
  | literal_pattern
  | tuple_pattern
  | struct_pattern
  | variant_pattern ;

tuple_pattern =
    "(", pattern, ",",
    [ pattern, { ",", pattern }, [ "," ] ],
    ")" ;

struct_pattern =
    path, "{",
    [ field_pattern, { ",", field_pattern }, [ "," ] ],
    "}" ;

field_pattern = IDENT, [ ":", pattern ] ;

variant_pattern =
    path,
    [ "(", [ pattern, { ",", pattern }, [ "," ] ], ")"
    | "{", [ field_pattern, { ",", field_pattern }, [ "," ] ], "}" ] ;
```

Whether `{ ... }` is a block, struct literal, pattern payload, trait body, or extension body is
determined by the construct that introduces it.

## 6. Paths

```ebnf
path =
    [ contextual("self") | "super" | "root" | IDENT ],
    { ".", IDENT } ;
```

The resolver, not the parser, determines whether the first segment names the current package, a
dependency package, or an entity in lexical scope.

## 7. Required Ambiguity Tests

The parser test suite must lock down at least these cases:

```sc fragment
let unit = ()
let singleton = (value,)
let grouped = (value)

f
(x)

let curried = make(T)(value)
let field = value.member
let chained = value?.member

if condition then { left() } else { right() }

match value {
  Some(item) -> item
} {
  None -> fallback
}
```

These examples distinguish unit from tuples, grouping from tuple syntax, a new statement from a
continued call, compile-time from runtime application, member access from conditional chaining,
and blocks from payload braces.
