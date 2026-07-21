let identity(T: type)(move value: T): T = { value }

let helper(value: i32) = { identity(i32)(value) }

let preserve(T: type)(move value: T): T = {
  helper(0)
  value
}

let main(): i32 = { preserve(i32)(42) }
