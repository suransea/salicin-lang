let inner = struct {
  value: i32,
}

let invalid = struct(c) {
  inner: inner,
}

let main(): i32 = { 0 }
