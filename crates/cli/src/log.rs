use xh_reports::{
    IntoReport,
    render::{PrettyRenderer, Render},
};

pub fn log_report<T>(report: &xh_reports::Report<T>) {
    eprintln!("{}", PrettyRenderer::new().render(&report));
}

pub struct Logger;

impl Logger {
    pub fn init() {
        log::set_max_level(log::LevelFilter::Debug);
        log::set_boxed_logger(Box::new(Logger) as _).expect("logger should not be set");
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.target().starts_with("xh")
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        log_report(&xh_reports::LogError::new(record).into_report());
    }

    fn flush(&self) {}
}
