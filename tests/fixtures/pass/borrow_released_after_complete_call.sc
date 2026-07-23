let Boxed = struct { value: i32 }

let read(boxed: borrow(Boxed)): i32 = { boxed.value }
let consume(move boxed: Boxed): i32 = { boxed.value }

let main(): i32 = {
  let boxed = Boxed { value: 42 }
  let snapshot = read(boxed)
  snapshot + consume(boxed) - 42
}
