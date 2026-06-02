.PHONY: build clean test

# Default target expected by the grading CI:
# - Compiles the workspace in release mode.
# - Copies the `hulk` binary to the repo root so `./hulk <file.hulk>` works.
build:
	cargo build --release --bin hulk
	cp target/release/hulk ./hulk

# Convenience: clean cargo artefacts and the dropped binary at the root.
clean:
	cargo clean
	rm -f ./hulk ./output

# Convenience: run the cargo test suite.
test:
	cargo test --workspace
