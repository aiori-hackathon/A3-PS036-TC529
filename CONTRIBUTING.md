# Contributing to CLIMB

This is a hackathon submission repository for AIORI-3 PS-036. At this time, external contributions are not being actively solicited.

## Development Guidelines

If you wish to build or extend this work:

- Follow the project guidelines and hard constraints summarised in the [README](README.md) and [architecture notes](docs/ARCHITECTURE.md)
- Respect the hard constraints: no new cryptography, no resolver implementation from scratch, no Internet deployment, no claimed security proofs
- Use only primitives from libMTL/liboqs/OpenSSL as provided
- Run `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --workspace` before submitting changes
- Record any benchmark or process change in the repository's progress log, following the project conventions

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
