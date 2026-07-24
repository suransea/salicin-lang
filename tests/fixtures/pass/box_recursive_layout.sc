let Option = std.Option

let Box = std.boxed.Box
let box_new = std.boxed.box_new

let Node = struct { value: i32, next: Option(Box(Node)) }

let main(): i32 = {
  let tail = Node { value: 42, next: None }
  let head = box_new(tail)
  42
}
