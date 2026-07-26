let opaque: type

let main(): i32 = { 42 }

test("opaque_type_declaration.sc") {
  main() == 42
}
