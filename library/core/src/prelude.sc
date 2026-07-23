pub let Never = enum {}

pub let Copy = trait {}

pub let Drop = trait {
  let drop(self: borrow(mut)(Self))
    (): ()
}
