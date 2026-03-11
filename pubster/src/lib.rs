pub mod proto {
    tonic::include_proto!("pubster");
}

pub mod client;
pub mod logger;
pub mod simulator;

pub type ThreadSafeError = Box<dyn std::error::Error + Send + Sync>;

#[macro_export]
macro_rules! log_line {
    ($($arg:tt)*) => {
        $crate::logger::write_line(&format!($($arg)*))
    };
}
