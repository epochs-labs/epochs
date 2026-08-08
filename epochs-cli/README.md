# epochs-cli

Command-line tool for [epochs](https://github.com/epochs-labs/epochs): `init`, `migrate`, `query`, `commit`, `branch`, `checkout`.

```bash
cargo install --path epochs-cli
epochs init
epochs migrate .
epochs query 'COMMIT { status: "ok" } MESSAGE "hi";'
```

**Stability:** 0.1.x

## License

MIT OR Apache-2.0
