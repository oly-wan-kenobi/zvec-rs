# zvec-derive

Derive macros for the [`zvec`](https://crates.io/crates/zvec) crate.

You almost certainly don't want this crate directly. Enable the
`derive` feature on `zvec` instead:

```toml
[dependencies]
zvec = { version = "0.1", features = ["derive"] }
```

and use `#[derive(IntoDoc)]` / `#[derive(FromDoc)]` re-exported from
`zvec`. See the [zvec crate
documentation](https://docs.rs/zvec) for the full attribute set and
usage examples.

## License

Apache-2.0, matching the parent crate.
