let Bag = struct { value: i32 }

let main(): i32 = {
  let bag = Bag { value: 42 }
  bag[0]
}
