let Option = std.Option

let next(): Option(i32) = { None }

let main(): i32 = {
  loop {
    match next()
      { Some(value) -> value }
      { None -> break() }
  }
  value
}
