let Boxed = struct { value: i32 }

let read(borrow boxed: Boxed): i32 = { boxed.value }

let main(): i32 = {
  let boxed = Boxed { value: 42 }
  let snapshot = read(boxed)
  snapshot + boxed.value - 42
}
