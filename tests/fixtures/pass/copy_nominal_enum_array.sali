let Mark = enum {
  Value(i32),
  Empty,
}

extend Mark: Copy {}

let Pixel = struct(value: i32)

extend Pixel: Copy {}

let score(mark: Mark): i32 = mark match {
  Mark.Value(value) => value,
  Mark.Empty => 0,
}

let main(): i32 = {
  let mark = Mark.Value(10)
  let pixels: Array(Pixel, 2) = [Pixel(20), Pixel(2)]
  score(mark) + score(mark) + pixels[0].value + pixels[1].value
}
