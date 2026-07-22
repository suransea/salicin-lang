let Produce = trait {
  let Item: type
  let produce(borrow self)(): Item
}

let Value = struct { value: i32 }

extend Value: Produce {
  let Item = i32
  let produce(borrow self)(): i32 = { self.value }
}

let produce(T: type)(borrow value: T): i32
where T: Produce(Item = i32) = { value.produce() }

let forward(T: type)(borrow value: T): i32
where T: Produce(Item = i32) = { produce(value) }

let main(): i32 = {
  let value = Value { value: 42 }
  forward(value)
}
