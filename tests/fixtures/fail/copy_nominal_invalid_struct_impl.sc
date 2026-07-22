let Payload = struct { value: i32 }

let Container = struct { payload: Payload }

extend Container: Copy {}

let main(): i32 = { 42 }
