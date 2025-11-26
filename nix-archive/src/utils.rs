pub struct TestingLogger;

#[cfg(feature = "log")]
#[allow(unused_imports)]
#[allow(dead_code)]
mod log {
    use super::TestingLogger;

    pub(crate) use log::{debug, error, info, trace, warn};

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
#[allow(unused_macros)]
#[allow(unused_imports)]
#[allow(dead_code)]
mod log {
    use super::TestingLogger;

    impl TestingLogger {
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

pub(crate) use log::*;

const PADDING: usize = 8;

#[inline]
pub fn calculate_padding(strlen: u64) -> usize {
    (PADDING - (strlen % PADDING as u64) as usize) % PADDING
}
