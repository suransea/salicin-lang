let consume(value: i16): i16 = { value }
let main(): i32 = {
  let narrow: i8 = 1
  consume(narrow)
  0
}
