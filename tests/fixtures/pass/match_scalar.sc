let classify(value: i32): i32 = { match value
  { -1 -> 1 }
  { number if number > 20 -> number }
  { _ -> 0 }
}

let select(value: bool): i32 = { match value
  { true -> 20 }
  { false -> 22 }
}

let main(): i32 = {
  classify(-1) + classify(41) + select(false) - 22
}
