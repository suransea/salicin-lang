let cell(comptime t: type) = struct { value: t }

extend(cell(t), copyable) {}

let main(): i32 = { 42 }
