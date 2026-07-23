use std.Option

let Inner = struct { answer: i32 }
let Middle = struct { inner: Inner }
let Outer = struct { middle: Middle }

let main(): i32 = {
  Option(Outer).Some(Outer { middle: Middle { inner: Inner { answer: 42 } } })?.middle?.inner?.answer ?? 0
}
