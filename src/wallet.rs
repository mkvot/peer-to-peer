use std::{
    fs::{self, File},
    io,
    path::Path,
    process,
};

use crate::{
    crypto::{now_millis, public_key_for_secret, seed_from_material},
    models::{Wallet, WalletFile},
};

pub fn load_or_create_wallet(data_dir: &Path) -> io::Result<Wallet> {
    fs::create_dir_all(data_dir)?;
    let path = data_dir.join("wallet.json");

    if path.exists() {
        let file = File::open(path)?;
        let wallet_file: WalletFile = serde_json::from_reader(file).map_err(invalid_data)?;
        validate_wallet(wallet_file)
    } else {
        let material = format!("{}:{}:{}", data_dir.display(), process::id(), now_millis());
        let secret_key = seed_from_material(&material);
        let public_key = public_key_for_secret(&secret_key).map_err(invalid_data)?;
        let wallet = WalletFile {
            public_key,
            secret_key,
        };
        let file = File::create(&path)?;
        serde_json::to_writer_pretty(file, &wallet).map_err(invalid_data)?;
        validate_wallet(wallet)
    }
}

fn validate_wallet(wallet: WalletFile) -> io::Result<Wallet> {
    let expected_public = public_key_for_secret(&wallet.secret_key).map_err(invalid_data)?;
    if wallet.public_key != expected_public {
        return Err(invalid_data("wallet public key does not match secret key"));
    }

    Ok(Wallet {
        public_key: wallet.public_key,
        secret_key: wallet.secret_key,
    })
}

fn invalid_data(error: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}
