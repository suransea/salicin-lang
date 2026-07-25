let Inner = struct {
  value: i32,
}

let Invalid = struct(c) {
  inner: Inner,
}

let main(): i32 = { 0 }
