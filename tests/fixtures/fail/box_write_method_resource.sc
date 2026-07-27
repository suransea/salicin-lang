let box = alloc.boxed.box

let resource = struct { value: i32 }

let main(): i32 = {
  box.new(resource { value: 1 }).write(resource { value: 2 })
  0
}
