pub let Option (T: type) = enum {
  Some(T),
  None,
}

pub let Result (T: type, E: type) = enum {
  Ok(T),
  Err(E),
}

pub let Never = enum {}

pub let Copy = trait {}

pub let Drop = trait {
  let drop(borrow(mut) self)(): ()
}
