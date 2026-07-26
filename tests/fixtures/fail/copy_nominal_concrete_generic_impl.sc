let cell(comptime t: type) = struct { value: t }

extend cell(i32): copyable {}

let read(copy cell: cell(i64)): i64 = { cell.value }

let main(): i32 = { 42 }
