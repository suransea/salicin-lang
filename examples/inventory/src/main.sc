let fail: with(std.io.io)(message: core.string.string)(code: i32): i32 = {
  let view = message.as_str()
  match std.io.eprintln(view)
    { ok(_) -> code }
    { err(_) -> code }
}

let take_number(
  arguments: borrow(mut)(alloc.vec.vec(core.string.string)),
): core.option(i64) = {
  let text = arguments.remove(1)
  let view = text.as_str()
  match parser.decimal(view)
    { ok(value) -> core.option.some(value) }
    { err(_) -> core.option.none }
}

let main: with(std.io.io)(): i32 = {
  let mut arguments = match std.io.arguments()
    { ok(value) -> value }
    { err(_) -> return(fail("arguments are not valid UTF-8")(2)) }
  if arguments.len() != 8 {
    return(fail("usage: inventory OUTPUT NAME UNITS PRICE NAME UNITS PRICE")(2))
  }

  let output_path = arguments.remove(1)
  let first_name = arguments.remove(1)
  let first_units = match take_number(arguments)
    { some(value) -> value }
    { none -> return(fail("invalid first units")(3)) }
  let first_price = match take_number(arguments)
    { some(value) -> value }
    { none -> return(fail("invalid first price")(3)) }
  let second_name = arguments.remove(1)
  let second_units = match take_number(arguments)
    { some(value) -> value }
    { none -> return(fail("invalid second units")(3)) }
  let second_price = match take_number(arguments)
    { some(value) -> value }
    { none -> return(fail("invalid second price")(3)) }

  let mut inventory = catalog.inventory.new()
  inventory.push(model.product.new(first_name, first_units, first_price))
  inventory.push(model.product.new(second_name, second_units, second_price))
  let text = report.render(inventory.summarize())
  let text_view = text.as_str()
  let bytes = text_view.as_bytes()
  let output_view = output_path.as_str()
  match std.io.write_file(output_view)(bytes)
    { err(_) -> return(fail("could not write output")(4)) }
    { ok(_) -> () }
  match std.io.print(text_view)
    { err(_) -> 5 }
    { ok(_) -> 0 }
}
