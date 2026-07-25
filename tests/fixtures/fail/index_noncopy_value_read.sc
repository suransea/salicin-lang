let Index = std.ops.Index

let Resource = struct { value: i32 }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {}
}

let Bag = struct { value: Resource }
extend Bag: Index(i32) {
  let Output = Resource
  let index(A: access)
    (self: borrow(A)(Self))
    (key: i32): borrow(A)(Resource) = {
    borrow(A)(self.value)
  }
}

let main(): Resource = {
  let bag = Bag { value: Resource { value: 42 } }
  bag[0]
}
