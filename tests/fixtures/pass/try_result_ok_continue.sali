let unwrap(move value: Result(i32, bool)): Result(i32, bool) = {
  let item = value.try
  item + 2
}

let main(): i32 = unwrap(Result(i32, bool).Ok(40)) ?? 0
