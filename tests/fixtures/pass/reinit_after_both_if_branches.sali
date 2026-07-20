let Boxed = struct(value: i32)

let consume(move boxed: Boxed): i32 = boxed.value

let restore(select_first: bool): i32 = {
  let mut boxed = Boxed(0)
  consume(boxed)
  if select_first {
    boxed = Boxed(20)
  } else {
    boxed = Boxed(22)
  }
  consume(boxed)
}

let main(): i32 = restore(true) + restore(false)
