let Number = struct(value: i32)

extend Number: Copy {}

let consume(move number: Number): i32 = { number.value }

let main(): i32 = {
  let mut number = Number(20)
  let first = consume(number)
  number = Number(22)
  first + consume(number)
}
