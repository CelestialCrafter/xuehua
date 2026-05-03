use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let timestamp = option_env!("SOURCE_DATE_EPOCH")
        .map(|str| str.parse().expect("could not parse SOURCE_DATE_EPOCH"))
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("should be able to subtract UNIX_EPOCH from current time")
                .as_secs()
        });

    println!("cargo:rustc-env=COMPILATION_SALT={timestamp}");
}
