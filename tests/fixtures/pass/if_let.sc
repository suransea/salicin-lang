let choose(value: Option(i32)): i32 = {
  if let Some(found) = value {
    found
  } else {
    2
  }
}

let main(): i32 = {
  choose(Some(40)) + choose(None)
}
