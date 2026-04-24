//! Print the runtime version reported by the linked `libzvec_c_api`.
//! A useful smoke test to confirm the library is wired up.
//!
//! Run with:
//!   cargo run --example version --features bundled

fn main() {
    println!("zvec version: {}", zvec::version());
    println!(
        "parsed: {}.{}.{}",
        zvec::version_major(),
        zvec::version_minor(),
        zvec::version_patch(),
    );
}
