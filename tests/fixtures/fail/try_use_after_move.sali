let reuse(move value: Option(i32)): Option(i32) = {
  let item = value.try
  let again = value match {
    Some(other) => other,
    None => 0,
  }
  item + again
}

let main(): i32 = reuse(Option(i32).Some(21)) ?? 0
