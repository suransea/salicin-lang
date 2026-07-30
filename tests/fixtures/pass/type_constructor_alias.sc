let cell(comptime t: type) = struct { value: t }

let family(comptime t: type): type = cell(t)
let constructor: (comptime t: type): type = cell
let scalar = i32

let main(): scalar = {
  let left: family(i32) = family(i32) { value: 41 }
  let right = constructor(i32) { value: 1 }
  left.value + right.value
}

test("type_constructor_alias.sc") {
  std.test.assert(main() == 42)
}
