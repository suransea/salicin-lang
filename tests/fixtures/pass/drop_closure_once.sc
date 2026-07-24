let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let consume(move value: Resource): () = { () }
let consume_pair(move left: Resource, move right: Resource): () = { () }

let invoke(): i32 = {
  let resource = Resource { value: 1 }
  let once = { consume(resource) }
  once()
  42
}

let abandon(): () = {
  let resource = Resource { value: 1 }
  let once = { consume(resource) }
}

let invoke_pair(): () = {
  let left = Resource { value: 1 }
  let right = Resource { value: 1 }
  let once = { consume_pair(left, right) }
  once()
}

let conditional(flag: bool): () = {
  let resource = Resource { value: 1 }
  let once = { consume(resource) }
  if flag { once() }
}

let early(): i32 = {
  let resource = Resource { value: 1 }
  let once = { (value: i32) -> do {
    consume(resource)
    value
  }}
  once(return(42))
}

let main(): i32 = {
  let answer = invoke()
  abandon()
  invoke_pair()
  conditional(true)
  conditional(false)
  answer + early() - 42
}
