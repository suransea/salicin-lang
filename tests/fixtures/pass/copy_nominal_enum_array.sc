let Mark = enum {
  Value( value: i32 ),
  Empty,
}

extend Mark: Copy {}

let Pixel = struct { value: i32 }

extend Pixel: Copy {}

let score(mark: Mark): i32 = { match mark
  { Mark.Value( value: value ) -> value }
  { Mark.Empty -> 0 }
}

let main(): i32 = {
  let mark = Mark.Value( value: 10 )
  let pixels: Array(Pixel, 2) = [Pixel { value: 20 }, Pixel { value: 2 }]
  score(mark) + score(mark) + pixels[0].value + pixels[1].value
}
