let number = struct { value: i32 }

extend number: copyable {}

let consume(move number: number): i32 = { number.value }

let main(): i32 = {
  let mut number = number { value: 20 }
  let first = consume(number)
  number = number { value: 22 }
  first + consume(number)
}

test("reinit_after_explicit_copy_move.sc") {
  main() == 42
}
