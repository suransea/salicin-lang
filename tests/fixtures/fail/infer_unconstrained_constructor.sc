let make(comptime f: (comptime t: type): type)(): i32 = { 42 }

let main(): i32 = { make() }
