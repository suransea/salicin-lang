let vec = alloc.vec.vec

/// Collection of owned products.
pub let inventory = struct {
  pub products: vec(model.product),
}

/// Consumes a collection and summarizes all entries through their trait API.
pub let summarize = trait {
  let summarize(move self)(): summary
}

pub let summary = struct {
  pub count: u64,
  pub total: i64,
  pub name_bytes: u64,
}

extend(inventory) {
  let new(): inventory = {
    inventory { products: vec(model.product).new() }
  }

  let push(self: borrow(mut)(self))(move product: model.product): () = {
    self.products.push(product)
  }
}

extend(inventory, summarize) {
  let summarize(move self)(): summary = {
    let mut owner = self
    let products = owner.products.take()
    let mut count: u64 = 0
    let mut total: i64 = 0
    let mut name_bytes: u64 = 0
    for products { product ->
      count = count + 1
      total = total + product.value()
      name_bytes = name_bytes + product.name_bytes()
    }
    summary { count: count, total: total, name_bytes: name_bytes }
  }
}

test("inventory combines arrays slices vectors and Unicode") {
  let expected_name_bytes: array(u64)(2) = [1, 3]
  let byte_view = expected_name_bytes.as_slice()
  let first_bytes: u64 = match byte_view.first()
    { some(value) -> value }
    { none -> std.test.fail("expected first byte count") }
  let last_bytes: u64 = match byte_view.last()
    { some(value) -> value }
    { none -> std.test.fail("expected last byte count") }
  std.test.assert_eq(u64)(first_bytes + last_bytes)(4)

  let mut value = inventory.new()
  value.push(model.product.new("A", 2, 10))
  value.push(model.product.new("柳", 3, 7))
  match value.summarize()
    { summary(count: count, total: total, name_bytes: name_bytes) ->
      std.test.assert(count == 2)
      std.test.assert(total == 41)
      std.test.assert(name_bytes == 4)
    }
}
