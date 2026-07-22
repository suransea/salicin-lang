// Compile-time domains used by parameter passing, regions, and borrow types.
pub let type = domain
pub let region = domain
pub let effect = domain

pub let access = domain {
  shared
  mut
}

pub let passing = domain {
  auto
  copy
  move
}
