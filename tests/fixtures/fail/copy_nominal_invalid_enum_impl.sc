let Payload = struct(value: i32)

let Message = enum {
  Data(Payload),
  Empty,
}

extend Message: Copy {}

let main(): i32 = 42
