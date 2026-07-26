let cell(comptime t: type) = struct { value: t }

extend(comptime t: type) cell(t): copyable {}

let main(): i32 = { 42 }
