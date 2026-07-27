let mark = enum {
  value( value: i32 ),
  empty,
}

extend(mark, copyable) {}

let pixel = struct { value: i32 }

extend(pixel, copyable) {}

let score(mark: mark): i32 = { match mark
    { mark.value( value: value ) -> value }
    { mark.empty -> 0 }
}

let main(): i32 = {
  let mark = mark.value( value: 10 )
  let pixels: array(pixel)(2) = [pixel { value: 20 }, pixel { value: 2 }]
  score(mark) + score(mark) + pixels[0].value + pixels[1].value
}

test("copy_nominal_enum_array.sc") {
  main() == 42
}
