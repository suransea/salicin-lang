let payload = struct { value: i32 }

let message = enum {
  data(payload),
  empty,
}

extend message: copyable {}

let main(): i32 = { 42 }
