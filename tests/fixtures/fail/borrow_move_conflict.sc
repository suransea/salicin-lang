let Boxed = struct(value: i32)

let consume(move boxed: Boxed): i32 = { boxed.value }

let main(): i32 = {
  let boxed = Boxed(value: 42)
  let borrowed = borrow boxed
  let answer = consume(boxed)
  borrowed.value + answer - 42
}
