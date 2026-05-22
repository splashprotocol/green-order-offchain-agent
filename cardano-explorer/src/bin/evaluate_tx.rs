use cardano_explorer::{ExtendedCardanoNetwork, Maestro, Network};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let key_path = args
        .next()
        .expect("usage: evaluate_tx <maestro-key-path> <tx-cbor-path>");
    let tx_path = args
        .next()
        .expect("usage: evaluate_tx <maestro-key-path> <tx-cbor-path>");
    let cbor = std::fs::read(tx_path)?;
    let cbor_hex = hex::encode(cbor);
    let explorer = Maestro::new(key_path, Network::Preprod).await?;
    let eval = explorer.evaluate_tx(&cbor_hex).await?;
    println!("{eval:#?}");
    Ok(())
}
