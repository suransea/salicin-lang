use std.Option

use std.boxed.{Box, box_new}

let Node = struct { value: i32, next: Option(Box(Node)) }

let main(): i32 = {
  let tail = Node { value: 42, next: None }
  let head = box_new(tail)
  42
}
