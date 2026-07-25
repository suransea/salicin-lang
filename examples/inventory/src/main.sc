let Result = std.Result
let String = std.string.String
let Vec = std.vec.Vec

let bytes(first: u8): Vec(u8) = {
  let mut values = Vec(u8).new()
  values.push(first)
  values
}

let valid_name(value: u8): String = {
  match parser.decode_name(bytes(value))
    { Ok(name) -> name }
    { Err(error) -> do {
      let original = error.into_bytes()
      String.new()
    } }
}

let invalid_input_is_recoverable(): bool = {
  match parser.decode_name(bytes(128))
    { Ok(_) -> false }
    { Err(error) -> do {
      let starts_invalid = error.valid_up_to() == 0
      let original = error.into_bytes()
      starts_invalid && original.len() == 1
    } }
}

let main(): i32 = {
  let first_name = valid_name(65)
  let second_name = valid_name(66)
  let names_are_present = first_name.len_bytes() == 1 && second_name.len_bytes() == 1

  let mut inventory = catalog.Inventory.new()
  inventory.push(model.Product.new(first_name, 2, 10))
  inventory.push(model.Product.new(second_name, 3, 7))
  let total = inventory.total()

  if names_are_present && invalid_input_is_recoverable() && total == 41 {
    42
  } else {
    1
  }
}
