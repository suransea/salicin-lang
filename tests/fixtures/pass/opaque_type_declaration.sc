let opaque: type

let main(): i32 = { 42 }

test("opaque_type_declaration.sc") {
  std.test.assert(main() == 42)
}
