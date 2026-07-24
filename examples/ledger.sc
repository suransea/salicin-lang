// A complete effectful batch-processing program over the frozen M0 core. The
// process exits with 42 after applying four valid transactions, or 1 on overdraft.

let Option = std.Option
let Iterator = std.iter.Iterator
let IntoIterator = std.iter.IntoIterator

let Transaction = enum {
  Credit(i32),
  Debit(i32),
}

let Overdraft = effect {
  let reject(): Never
}

let Ledger = struct {
  balance: i32,
  processed: i32,
}

let Account = trait {
  let credit(self: borrow(mut)(Self))(amount: i32): ()
  let debit(self: borrow(mut)(Self))(amount: i32): ()
  let snapshot(self: borrow(Self))(): i32
}

extend Ledger: Account {
  let credit(self: borrow(mut)(Self))(amount: i32): () = {
    self.balance = self.balance + amount
    self.processed = self.processed + 1
  }

  let debit(self: borrow(mut)(Self))(amount: i32): () = {
    self.balance = self.balance - amount
    self.processed = self.processed + 1
  }

  let snapshot(self: borrow(Self))(): i32 = {
    if self.processed == 4 { self.balance } else { 0 }
  }
}

let Batch = struct {
  index: i32,
}

extend Batch: Iterator {
  let Item = i32

  let next(self: borrow(mut)(Self))(): Option(i32) = {
    let transaction: Option(i32) = match self.index
      { 0 -> Some(30) }
      { 1 -> Some(8) }
      { 2 -> Some(25) }
      { 3 -> Some(5) }
      { _ -> None }
    self.index = self.index + 1
    transaction
  }
}

extend Batch: IntoIterator {
  let IntoIter = Batch

  let into_iter(move self)(): Batch = {
    self
  }
}

let count_batch(move batch: Batch): i32 = {
  let mut count = 0
  for batch { _ ->
    count = count + 1
  }
  count
}

let amount(move transaction: Transaction): i32 = {
  match transaction
    { Credit(value) -> value }
    { Debit(value) -> value }
}

let process(first_credit: i32)(first_debit: i32)(second_credit: i32)(second_debit: i32): i32 with(Overdraft) = {
  let mut ledger = Ledger { balance: 0, processed: 0 }
  ledger.credit(first_credit)
  if first_debit > ledger.balance {
    Overdraft.reject()
  } else {
    ledger.debit(first_debit)
  }
  ledger.credit(second_credit)
  if second_debit > ledger.balance {
    Overdraft.reject()
  } else {
    ledger.debit(second_debit)
  }
  ledger.snapshot()
}

let main(): i32 = {
  let first = amount(Transaction.Credit(30))
  let second = amount(Transaction.Debit(8))
  let third = amount(Transaction.Credit(25))
  let fourth = amount(Transaction.Debit(5))
  let balance = Overdraft.handle reject { () ->
    1
  } action {
    process(first)(second)(third)(fourth)
  }
  let count = count_batch(Batch { index: 0 })
  if balance == 42 && count == 4 { 42 } else { 1 }
}
