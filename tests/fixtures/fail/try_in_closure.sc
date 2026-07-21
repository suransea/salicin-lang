let main(): i32 = {
  let unwrap = { (move value: Option(i32)) -> value.try }
  unwrap(Option(i32).Some(42)) ?? 0
}
