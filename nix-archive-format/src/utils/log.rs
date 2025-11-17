pub struct TestingLogger;

#[cfg(feature = "log")]
mod inner {
    use super::TestingLogger;

    pub use log::{debug, error, info, trace, warn};

    impl TestingLogger {
        #[inline]
        pub fn init() {
            if let Err(_) = log::set_logger(&TestingLogger) {
                log::warn!("logger already registered, cannot register TestingLogger");
            }

            log::set_max_level(log::LevelFilter::Trace);
        }
    }

    impl log::Log for TestingLogger {
        #[inline]
        fn enabled(&self, _metadata: &log::Metadata) -> bool {
            true
        }

        #[inline]
        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                eprintln!(
                    "({}) ({}) {}",
                    record.module_path().unwrap_or("test"),
                    record.level(),
                    record.args()
                );
            }
        }

        fn flush(&self) {}
    }
}

#[cfg(not(feature = "log"))]
mod inner {
    impl super::TestingLogger {
        #[inline]
        pub fn init() {}
    }

    macro_rules! error {
        ($($x:tt)*) => {};
    }

    macro_rules! _warn {
        ($($x:tt)*) => {};
    }

    macro_rules! info {
        ($($x:tt)*) => {};
    }

    macro_rules! debug {
        ($($x:tt)*) => {};
    }

    macro_rules! trace {
        ($($x:tt)*) => {};
    }

    pub(crate) use {_warn as warn, debug, error, info, trace};
}

pub use inner::*;
