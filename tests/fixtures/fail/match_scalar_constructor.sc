let Value = enum { Number(i32) }

let main(): i32 = { 42 match {
  Value.Number(value) => value,
  _ => 0,
}
}
