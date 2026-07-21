let classify(value: u32): i32 = value match {
  -1 => 1,
  _ => 0,
}

let main(): i32 = classify(42)
