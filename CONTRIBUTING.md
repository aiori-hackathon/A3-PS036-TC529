# Contributing to CLIMB

This is a hackathon submission repository for AIORI-3 PS-036. At this time, external contributions are not being actively solicited.

## Development Guidelines

If you wish to build or extend this work:

- Follow the guidelines in [`the project conventions`](the project conventions) and [`the design notes`](the design notes)
- Respect the hard constraints: no new cryptography, no resolver implementation from scratch, no Internet deployment, no claimed security proofs
- Use only primitives from libMTL/liboqs/OpenSSL as provided
- Run `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --workspace` before submitting changes
- Update `the dev notes` for any phase/work completed following the project conventions

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
