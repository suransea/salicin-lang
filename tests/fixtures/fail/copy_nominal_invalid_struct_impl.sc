let payload = struct { value: i32 }

let container = struct { payload: payload }

extend(container, copyable) {}

let main(): i32 = { 42 }
