use log::{LevelFilter, Log, Metadata, Record, set_logger, set_max_level, warn};

// TestingLogger is used exclusively in tests, so rust treats it as dead code
#[allow(dead_code)]
pub struct TestingLogger;

impl TestingLogger {
    // same reason as above
    #[allow(dead_code)]
    pub fn init() {
        if let Err(_) = set_logger(&TestingLogger) {
            warn!("logger already registered, cannot register TestingLogger");
        }

        set_max_level(LevelFilter::Trace);
    }
}

impl Log for TestingLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
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

pub const PADDING: usize = 8;
pub fn calculate_padding(strlen: u64) -> usize {
    (PADDING - (strlen % PADDING as u64) as usize) % PADDING
}
