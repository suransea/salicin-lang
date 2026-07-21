let Inner = struct(answer: i32)
let Middle = struct(inner: Inner)
let Outer = struct(middle: Middle)

let main(): i32 =
  Option(Outer).Some(Outer(Middle(Inner(42))))?.middle?.inner?.answer ?? 0
