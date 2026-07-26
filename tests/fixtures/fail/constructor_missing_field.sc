let pair = struct { left: i32, right: i32 }

let main(): i32 = {
  let pair = pair { left: 42 }
  pair.left
}
