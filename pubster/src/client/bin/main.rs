use::pubster::control;

pub mod control {
    tonic_build::include_proto!("pubster");
}

fn main() {
    println!("I am a client");
}