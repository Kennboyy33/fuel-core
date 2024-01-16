use anyhow::{
    anyhow,
    Context,
    Error,
};
use graph::{
    blockchain::{
        self,
        Block,
        Blockchain,
        TriggerWithHandler,
    },
    components::store::StoredDynamicDataSource,
    data::subgraph::DataSourceContext,
    prelude::{
        async_trait,
        BlockNumber,
        CheapClone,
        DataSourceTemplateInfo,
        Deserialize,
        Link,
        LinkResolver,
        Logger,
        SubgraphManifestValidationError,
    },
    semver,
};
use std::{
    collections::HashSet,
    sync::Arc,
};

use crate::{
    chain::Chain,
    codec,
};

pub const FUEL_KIND: &str = "fuel";
const BLOCK_HANDLER_KIND: &str = "block";
const RECEIPT_HANDLER_KIND: &str = "receipt";

/// Runtime representation of a data source.
#[derive(Clone, Debug)]
pub struct DataSource {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub(crate) source: Source,
    pub mapping: Mapping,
    pub context: Arc<Option<DataSourceContext>>,
    pub creation_block: Option<BlockNumber>,
}

impl DataSource {
    fn from_manifest(
        kind: String,
        network: Option<String>,
        name: String,
        source: Source,
        mapping: Mapping,
        context: Option<DataSourceContext>,
    ) -> Result<Self, Error> {
        // Data sources in the manifest are created "before genesis" so they have no creation block.
        let creation_block = None;

        Ok(DataSource {
            kind,
            network,
            name,
            source,
            mapping,
            context: Arc::new(context),
            creation_block,
        })
    }

    fn handler_for_block(&self) -> Option<&MappingBlockHandler> {
        self.mapping.block_handlers.first()
    }

    // fn handler_for_receipt(&self) -> Option<&ReceiptHandler> {
    //     self.mapping.receipt_handlers.first()
    // }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct ReceiptHandler {
    pub(crate) handler: String,
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedMapping {
    pub api_version: String,
    pub language: String,
    pub entities: Vec<String>,
    #[serde(default)]
    pub block_handlers: Vec<MappingBlockHandler>,
    #[serde(default)]
    pub receipt_handlers: Vec<ReceiptHandler>,
    pub file: Link,
}

impl UnresolvedMapping {
    pub async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
    ) -> Result<Mapping, Error> {
        let UnresolvedMapping {
            api_version,
            language,
            entities,
            block_handlers,
            receipt_handlers,
            file: link,
        } = self;

        let api_version = semver::Version::parse(&api_version)?;

        let module_bytes = resolver
            .cat(logger, &link)
            .await
            .with_context(|| format!("failed to resolve mapping {}", link.link))?;

        Ok(Mapping {
            api_version,
            language,
            entities,
            block_handlers,
            runtime: Arc::new(module_bytes),
            link,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct UnresolvedDataSource {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub(crate) source: Source,
    pub mapping: UnresolvedMapping,
    pub context: Option<DataSourceContext>,
}

#[async_trait]
impl blockchain::UnresolvedDataSource<Chain> for UnresolvedDataSource {
    async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
        _manifest_idx: u32,
    ) -> Result<DataSource, Error> {
        let UnresolvedDataSource {
            kind,
            network,
            name,
            source,
            mapping,
            context,
        } = self;

        let mapping = mapping.resolve(resolver, logger).await.with_context(|| {
            format!(
                "failed to resolve data source {} with source_account {:?} and source_start_block {}",
                name, source.owner, source.start_block
            )
        })?;

        DataSource::from_manifest(kind, network, name, source, mapping, context)
    }
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize)]
pub struct BaseDataSourceTemplate<M> {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub mapping: M,
}

pub type UnresolvedDataSourceTemplate = BaseDataSourceTemplate<UnresolvedMapping>;
pub type DataSourceTemplate = BaseDataSourceTemplate<Mapping>;

#[async_trait]
impl blockchain::UnresolvedDataSourceTemplate<Chain> for UnresolvedDataSourceTemplate {
    async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
        _manifest_idx: u32,
    ) -> Result<DataSourceTemplate, Error> {
        let UnresolvedDataSourceTemplate {
            kind,
            network,
            name,
            mapping,
        } = self;

        let mapping = mapping.resolve(resolver, logger).await.with_context(|| {
            format!("failed to resolve data source template {}", name)
        })?;

        Ok(DataSourceTemplate {
            kind,
            network,
            name,
            mapping,
        })
    }
}

impl blockchain::DataSourceTemplate<Chain> for DataSourceTemplate {
    fn api_version(&self) -> semver::Version {
        self.mapping.api_version.clone()
    }

    fn runtime(&self) -> Option<Arc<Vec<u8>>> {
        Some(self.mapping.runtime.cheap_clone())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn manifest_idx(&self) -> u32 {
        unreachable!("fuel does not support dynamic data sources")
    }

    fn kind(&self) -> &str {
        &self.kind
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Source {
    // A data source that does not have an owner can only have block handlers.
    pub(crate) owner: Option<String>,
    #[serde(default)]
    pub(crate) start_block: BlockNumber,
    pub(crate) end_block: Option<BlockNumber>,
}

#[derive(Clone, Debug)]
pub struct Mapping {
    pub api_version: semver::Version,
    pub language: String,
    pub entities: Vec<String>,
    pub block_handlers: Vec<MappingBlockHandler>,
    pub runtime: Arc<Vec<u8>>,
    pub link: Link,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct MappingBlockHandler {
    pub handler: String,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct TransactionHandler {
    pub handler: String,
}

impl blockchain::DataSource<Chain> for DataSource {
    fn from_template_info(info: DataSourceTemplateInfo<Chain>) -> Result<Self, Error> {
        Err(anyhow!("Fuel subgraphs do not support templates"))
    }

    fn from_stored_dynamic_data_source(
        template: &DataSourceTemplate,
        stored: StoredDynamicDataSource,
    ) -> Result<Self, Error> {
        todo!()
    }

    fn address(&self) -> Option<&[u8]> {
        self.source.owner.as_ref().map(String::as_bytes)
    }

    fn start_block(&self) -> BlockNumber {
        self.source.start_block
    }

    fn end_block(&self) -> Option<BlockNumber> {
        self.source.end_block
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> &str {
        &self.kind
    }

    fn network(&self) -> Option<&str> {
        self.network.as_deref()
    }

    fn context(&self) -> Arc<Option<DataSourceContext>> {
        self.context.cheap_clone()
    }

    fn creation_block(&self) -> Option<BlockNumber> {
        self.creation_block
    }

    fn api_version(&self) -> semver::Version {
        self.mapping.api_version.clone()
    }

    fn runtime(&self) -> Option<Arc<Vec<u8>>> {
        Some(self.mapping.runtime.cheap_clone())
    }

    fn handler_kinds(&self) -> HashSet<&str> {
        let mut kinds = HashSet::new();

        if self.handler_for_block().is_some() {
            kinds.insert(BLOCK_HANDLER_KIND);
        }

        // if self.handler_for_receipt().is_some() {
        //     kinds.insert(RECEIPT_HANDLER_KIND);
        // }

        kinds
    }
    // Todo Emir
    fn match_and_decode(
        &self,
        trigger: &<Chain as Blockchain>::TriggerData,
        block: &Arc<<Chain as Blockchain>::Block>,
        _logger: &Logger,
    ) -> Result<Option<TriggerWithHandler<Chain>>, Error> {
        if self.source.start_block > block.number() {
            return Ok(None);
        }

        let handler = match self.handler_for_block() {
            Some(handler) => &handler.handler,
            None => return Ok(None),
        };

        Ok(Some(TriggerWithHandler::<Chain>::new(
            trigger.cheap_clone(),
            handler.clone(),
            block.ptr(),
        )))
    }

    fn is_duplicate_of(&self, other: &Self) -> bool {
        let DataSource {
            kind,
            network,
            name,
            source,
            mapping,
            context,

            // The creation block is ignored for detection duplicate data sources.
            // Contract ABI equality is implicit in `source` and `mapping.abis` equality.
            creation_block: _,
        } = self;

        // mapping_request_sender, host_metrics, and (most of) host_exports are operational structs
        // used at runtime but not needed to define uniqueness; each runtime host should be for a
        // unique data source.
        kind == &other.kind
            && network == &other.network
            && name == &other.name
            && source == &other.source
            && mapping.block_handlers == other.mapping.block_handlers
            && context == &other.context
    }

    fn as_stored_dynamic_data_source(&self) -> StoredDynamicDataSource {
        todo!()
    }

    fn validate(&self) -> Vec<Error> {
        let mut errors = Vec::new();

        if self.kind != FUEL_KIND {
            errors.push(anyhow!(
                "data source has invalid `kind`, expected {} but found {}",
                FUEL_KIND,
                self.kind
            ))
        }

        // Validate that there is a `source` address if there are receipt handlers
        let no_source_address = self.address().is_none();

        // Validate not both address and partial addresses are empty.
        if no_source_address {
            errors.push(SubgraphManifestValidationError::SourceAddressRequired.into());
        };

        // Validate that there are no more than one of both block handlers and receipt handlers
        if self.mapping.block_handlers.len() > 1 {
            errors.push(anyhow!("data source has duplicated block handlers"));
        }

        errors
    }
}

pub struct ReceiptWithOutcome {
    // REVIEW: Do we want to actually also have those two below behind an `Arc` wrapper?
    // pub outcome: codec::ExecutionOutcomeWithId,
    // pub receipt: codec::Receipt,
    pub block: Arc<codec::Block>,
}
