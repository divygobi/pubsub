pub mod proto {
    tonic::include_proto!("pubster");
}

pub mod client;
pub mod logger;
pub mod simulator;

#[macro_export]
macro_rules! log_line {
    ($($arg:tt)*) => {
        $crate::logger::write_line(&format!($($arg)*))
    };
}
