let parse_i64_radix = core.fmt.parse_i64_radix

/// Parses one strict decimal command-line field.
pub let decimal(
  value: borrow(core.string.str),
): core.result(core.fmt.parse_int_error)(i64) = {
  parse_i64_radix(value, 10)
}

test("decimal parser accepts signed input") {
  let input: string = "-17"
  let view = input.as_str()
  match decimal(view)
    { ok(value) -> std.test.assert(value == -17) }
    { err(_) -> std.test.fail("expected parsed -17") }
}

test("decimal parser rejects trailing text") {
  let input: string = "12x"
  let view = input.as_str()
  match decimal(view)
    { ok(_) -> std.test.fail("expected invalid_digit") }
    { err(error) ->
      match error.kind()
      { invalid_digit -> () }
      { _ -> std.test.fail("expected invalid_digit") }
    }
}
