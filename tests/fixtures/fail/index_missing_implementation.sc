let bag = struct { value: i32 }

let main(): i32 = {
  let bag = bag { value: 42 }
  bag[0]
}
