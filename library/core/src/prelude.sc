pub let Never = enum {}

pub let Copy = trait {}

pub let Drop = trait {
  let drop(borrow(mut) self)(): ()
}
