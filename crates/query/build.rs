fn main() {
    let salt = getrandom::u64().expect("could not generate randomness for compilation salt");
    println!("cargo:rustc-env=COMPILATION_SALT={salt}");
}
