let box = alloc.boxed.box

let resource = struct { value: i32 }

let main(): i32 = { box.new(resource { value: 42 }).read().value }
