.PHONY: user

# Compile user programs in src/bin
user: user/exec

user/% : src/bin/%.rs makefile
	cargo rustc --release --bin $* -- \
		-C linker-flavor=ld \
		-C link-args="-Ttext-segment=5000000 -Trodata-segment=5100000" \
		-C relocation-model=static
	mkdir -p user
	cp target/x86_64-zephyrRS/release/$* user/

run : user
	cargo run --bin zephyRS
