use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct NodeConfig {
    pub difficulty: u32,
    pub data_dir_base: PathBuf,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            difficulty: 4,
            data_dir_base: PathBuf::from("ledger_data"),
        }
    }
}
