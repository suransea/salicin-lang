let Option = std.Option

let Box = std.boxed.Box

let Node = struct { value: i32, next: Option(Box(Node)) }

let main(): i32 = {
  let tail = Node { value: 42, next: None }
  let head = Box.new(tail)
  42
}
