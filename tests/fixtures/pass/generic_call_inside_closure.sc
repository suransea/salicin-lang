let identity(T: type)(move value: T): T = { value }

let through_closure(T: type)(move value: T): T = {
  let apply = { (item: T) -> identity(T)(item) }
  apply(value)
}

let main(): i32 = { through_closure(i32)(42) }
