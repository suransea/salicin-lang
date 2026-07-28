let main(): i32 = {
  let first_name = parser.decode_name("A")
  let second_name = parser.decode_name("柳")
  let names_are_present =
    first_name.len_bytes() == 1 && second_name.len_bytes() == 3

  let mut inventory = catalog.inventory.new()
  inventory.push(model.product.new(first_name, 2, 10))
  inventory.push(model.product.new(second_name, 3, 7))
  let total = inventory.total()

  if names_are_present && total == 41 {
    42
  } else {
    1
  }
}
