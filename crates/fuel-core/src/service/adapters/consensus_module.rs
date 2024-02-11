use crate::{
    database::Database,
    service::{
        adapters::{
            MaybeRelayerAdapter,
            VerifierAdapter,
        },
        Config,
    },
};
use fuel_core_consensus_module::block_verifier::{
    config::Config as VerifierConfig,
    Verifier,
};
use fuel_core_poa::ports::RelayerPort;
use fuel_core_producer::ports::BlockProducerDatabase;
use fuel_core_storage::{
    tables::FuelBlocks,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        header::BlockHeader,
        primitives::DaBlockHeight,
    },
    fuel_tx::Bytes32,
    fuel_types::BlockHeight,
};
use std::sync::Arc;

pub mod poa;

impl VerifierAdapter {
    pub fn new(config: &Config, database: Database) -> Self {
        let block_height = config.state_reader.block_height();
        let config = VerifierConfig::new(config.chain_config.clone(), block_height);
        Self {
            block_verifier: Arc::new(Verifier::new(config, database)),
        }
    }
}

impl fuel_core_poa::ports::Database for Database {
    fn block_header(&self, height: &BlockHeight) -> StorageResult<BlockHeader> {
        Ok(self.get_block(height)?.header().clone())
    }

    fn block_header_merkle_root(&self, height: &BlockHeight) -> StorageResult<Bytes32> {
        self.storage::<FuelBlocks>().root(height).map(Into::into)
    }
}

#[async_trait::async_trait]
impl RelayerPort for MaybeRelayerAdapter {
    async fn await_until_if_in_range(
        &self,
        da_height: &DaBlockHeight,
        _max_da_lag: &DaBlockHeight,
    ) -> anyhow::Result<()> {
        #[cfg(feature = "relayer")]
        {
            if let Some(sync) = self.relayer_synced.as_ref() {
                let current_height = sync.get_finalized_da_height()?;
                anyhow::ensure!(
                    da_height.saturating_sub(*current_height) <= **_max_da_lag,
                    "Relayer is too far out of sync"
                );
                sync.await_at_least_synced(da_height).await?;
            }
            Ok(())
        }
        #[cfg(not(feature = "relayer"))]
        {
            anyhow::ensure!(
                **da_height == 0,
                "Cannot have a da height above zero without a relayer"
            );
            Ok(())
        }
    }
}
