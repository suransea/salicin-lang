let Boxed = struct(value: i32)

let consume(move boxed: Boxed): i32 = boxed.value

let choose(take: bool): i32 = {
  let boxed = Boxed(value: 42)
  if take {
    return consume(boxed)
  }
  boxed.value
}

let main(): i32 = choose(false)
