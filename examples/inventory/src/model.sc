let string = core.string.string

/// Product data owns its validated UTF-8 name.
pub let product = struct {
  pub name: string,
  pub units: i64,
  pub unit_price: i64,
}

/// Computes a value without exposing a product's representation.
pub let valued = trait {
  let value(self: borrow(self))(): i64
}

extend(product) {
  let new(move name: string, units: i64, unit_price: i64): product = {
    product { name: name, units: units, unit_price: unit_price }
  }

  let name_bytes(self: borrow(self))(): u64 = {
    self.name.len_bytes()
  }
}

extend(product, valued) {
  let value(self: borrow(self))(): i64 = {
    self.units * self.unit_price
  }
}
