let vec = std.vec.vec

/// Collection of owned products.
pub(package) let inventory = struct {
  products: vec(model.product),
}

/// Consumes a collection and summarizes all entries through their trait API.
pub(package) let summarize = trait {
  let total(move self)(): i32
}

extend inventory {
  let new(): inventory = {
    inventory { products: vec(model.product).new() }
  }

  let push(self: borrow(mut)(self))(move product: model.product): () = {
    self.products.push(product)
  }
}

extend inventory: summarize {
  let total(move self)(): i32 = {
    let mut owner = self
    let products = owner.products.take()
    let mut total = 0
    for products { product ->
      total = total + product.value()
    }
    total
  }
}
