// Private bootstrap marker for declarations whose definitions are supplied by
// the compiler. Semantic validation gives each use its declaration annotation.
let builtin(): Never = builtin()

pub let Never = core.never.Never
pub let Move = core.marker.Move
pub let Copy = core.marker.Copy
pub let Drop = core.marker.Drop
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
pub let Option = core.option.Option
pub let Result = core.result.Result
pub let Array = core.memory.Array
pub let Slice = core.memory.Slice
pub let Ptr = core.memory.Ptr
pub let size_of = core.memory.size_of
pub let align_of = core.memory.align_of
