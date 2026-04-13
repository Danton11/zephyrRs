fn main() {
    // Pin the user binary's load address to match USER_CODE_START in
    // kernel/src/proc/process.rs (0x5000000). rust-lld / lld-link only
    // support --image-base; rodata naturally follows text so we no longer
    // set it explicitly.
    println!("cargo:rustc-link-arg=--image-base=0x5000000");
}
