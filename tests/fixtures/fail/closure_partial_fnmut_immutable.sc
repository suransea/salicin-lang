let main(): i32 = {
  let mut total = 40
  let mut add = { (x: i32)(y: i32) ->
    total = total + x + y
    total
  }
  let add_one = add(1)
  add_one(1)
}
