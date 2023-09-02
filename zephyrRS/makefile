
user/% : FORCE
	cargo build --release --bin $*
	mkdir -p user
	cp target/x86_64-zephyr/release/$* user/

FORCE:

.PHONY: run
run : user/bin
	cargo run --release --bin kernel
