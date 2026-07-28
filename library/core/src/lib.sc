// Private bootstrap marker for declarations whose definitions are supplied by
// the compiler. Semantic validation gives each use its declaration annotation.
let builtin() = builtin()

// Public syntax contracts. Their leading groups are erased metadata.
pub let abi = core.foreign.abi
pub let foreign = core.foreign.foreign
// Test names are consumed by the `test("...") { ... }` syntax. The boolean
// action is the only value passed to this compiler-supplied contract.
pub let test(move body: (): bool): () = builtin()

pub let never = core.never.never
pub let movable = core.marker.movable
pub let copyable = core.marker.copyable
pub let droppable = core.marker.droppable
pub let bool = core.primitives.bool
pub let i8 = core.primitives.i8
pub let i16 = core.primitives.i16
pub let i32 = core.primitives.i32
pub let i64 = core.primitives.i64
pub let i128 = core.primitives.i128
pub let isize = core.primitives.isize
pub let u8 = core.primitives.u8
pub let u16 = core.primitives.u16
pub let u32 = core.primitives.u32
pub let u64 = core.primitives.u64
pub let u128 = core.primitives.u128
pub let usize = core.primitives.usize
pub let option = core.option.option
pub let result = core.result.result
pub let array = core.memory.array
pub let slice = core.memory.slice
pub let ptr = core.memory.ptr
pub let size_of = core.memory.size_of
pub let align_of = core.memory.align_of
pub let string = core.string.string
pub let unicode_scalar = core.string.unicode_scalar
pub let array_literal = core.literal.array_literal
pub let string_literal = core.literal.string_literal
