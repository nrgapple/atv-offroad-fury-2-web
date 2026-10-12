//! Native test fixture bridge: exercise live sockets with the actual Rust codec.
use std::io::{self, Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    if std::env::args().nth(1).as_deref() == Some("decode") {
        let input = input.trim();
        let bytes = (0..input.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&input[at..at + 2], 16))
            .collect::<Result<Vec<_>, _>>()?;
        println!("{}", serde_json::to_string(&atv_net::decode(&bytes)?)?);
    } else {
        let packet: atv_net::Packet = serde_json::from_str(&input)?;
        let bytes = atv_net::encode(&packet)?;
        println!(
            "{}",
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
    }
    Ok(())
}
