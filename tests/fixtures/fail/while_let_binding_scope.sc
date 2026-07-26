let option = std.option

let next(): option(i32) = { none }

let main(): i32 = {
  loop {
    match next()
      { some(value) -> value }
      { none -> break() }
  }
  value
}
