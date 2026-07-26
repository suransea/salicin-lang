let result = std.result
let string = std.string.string
let vec = std.vec.vec

let bytes(first: u8): vec(u8) = {
  let mut values = vec(u8).new()
  values.push(first)
  values
}

let valid_name(value: u8): string = {
  match parser.decode_name(bytes(value))
    { ok(name) -> name }
    { err(error) -> do {
      let original = error.into_bytes()
      string.new()
    } }
}

let invalid_input_is_recoverable(): bool = {
  match parser.decode_name(bytes(128))
    { ok(_) -> false }
    { err(error) -> do {
      let starts_invalid = error.valid_up_to() == 0
      let original = error.into_bytes()
      starts_invalid && original.len() == 1
    } }
}

let main(): i32 = {
  let first_name = valid_name(65)
  let second_name = valid_name(66)
  let names_are_present = first_name.len_bytes() == 1 && second_name.len_bytes() == 1

  let mut inventory = catalog.inventory.new()
  inventory.push(model.product.new(first_name, 2, 10))
  inventory.push(model.product.new(second_name, 3, 7))
  let total = inventory.total()

  if names_are_present && invalid_input_is_recoverable() && total == 41 {
    42
  } else {
    1
  }
}
