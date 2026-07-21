use alloc.vec.{Vec, vec_at}

let Resource = struct(value: i32)

extend Resource {
  let read(borrow self)(): i32 = self.value
}

let main(): i32 = {
  let mut values: Vec(Resource) = Vec(Resource).new()
  values.push(Resource(20))
  values.push(Resource(0))
  let first = do {
    let reference = vec_at(values)(0)
    reference.read()
  }
  do {
    let reference = vec_at(A: mut)(values)(1)
    reference.value = 21
  }
  let second = do {
    let reference = values.at(1)
    reference.read()
  }
  do {
    let reference = values.at(mut)(1)
    reference.value = 22
  }
  let third = do {
    let reference = values.at(1)
    reference.read()
  }
  if second == 21 { first + third } else { 0 }
}
