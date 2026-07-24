let main(): i32 = {
  let mut total = 39
  let mut add = { (x: i32)(y: i32) ->
    total = total + x + y
    total
  }
  let mut add_one = add(1)
  let first = add_one(1)
  let second = add_one(1)
  first + second - 42
}
