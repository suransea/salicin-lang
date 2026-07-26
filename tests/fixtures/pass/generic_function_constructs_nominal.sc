let cell(comptime t: type) = struct { value: t }

let wrap(comptime t: type)(move value: t): cell(t) = { cell(t) { value: value } }

let main(): i32 = {
  let wrapped = wrap(i32)(42)
  wrapped.value
}

test("generic_function_constructs_nominal.sc") {
  main() == 42
}
