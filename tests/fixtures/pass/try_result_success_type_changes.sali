let is_answer(move value: Result(i32, bool)): Result(bool, bool) = {
  let item = value.try
  item == 42
}

let main(): i32 = {
  let answer = is_answer(Result(i32, bool).Ok(42)) ?? false
  if answer { 42 } else { 0 }
}
