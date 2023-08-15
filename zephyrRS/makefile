
user/% : FORCE
	cargo build --release --bin $*
	mkdir -p user
	cp target/x86_64-zephyr/release/$* user/

FORCE:

.PHONY: run
run : user/bin user/shell
	cargo run --release --bin kernel
