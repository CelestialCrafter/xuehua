fn main() {
    #[cfg(feature = "bubblewrap-builder")]
    {
        use std::{env, process::Command};

        println!("cargo::rerun-if-changed=../cmd-runner");
        let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR should be set");

        let status = Command::new("go")
            .args(&["build", "-C", "../cmd-runner", "-o"])
            .arg(out_dir)
            .arg(".")
            // TODO: remove env var when jsonv2 becomes stable
            .env("GOEXPERIMENT", "jsonv2")
            .status()
            .expect("building cmd-runner should not fail");
        assert!(status.success());
    }
}
