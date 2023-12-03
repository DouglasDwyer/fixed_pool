# fixed_pool

[![Crates.io](https://img.shields.io/crates/v/fixed_pool.svg)](https://crates.io/crates/fixed_pool)
[![Docs.rs](https://docs.rs/fixed_pool/badge.svg)](https://docs.rs/fixed_pool)

`fixed_pool` implements an object pool with a fixed number of items. The items may be borrowed without lifetime restrictions,
are automatically returned to the pool upon drop, and may have customized reset semantics through the use of a trait.