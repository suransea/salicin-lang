let Boxed = struct(value: i32)

let consume(move boxed: Boxed): i32 = boxed.value

let main(): i32 = {
  let mut boxed = Boxed(14)
  let first = consume(boxed)
  boxed = Boxed(14)
  let read = boxed.value
  let second = consume(boxed)
  first + read + second
}
