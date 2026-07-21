let unwrap(move value: Option(i32)): Option(i32) = {
  let item = value.try
  item
}

let main(): i32 = unwrap(Option(i32).Some(42)) ?? 0
