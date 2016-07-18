build:
	cargo build && \
	cd applications && \
	cargo build

check:
	cargo check && \
	cd applications && \
	cargo check

test:
	cargo test && \
	cd applications && \
	cargo test
