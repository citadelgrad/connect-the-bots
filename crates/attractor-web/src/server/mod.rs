// These modules contain server functions that need to be available in both ssr and hydrate modes
// The #[server] macro generates client stubs for hydrate
pub mod generate;
pub mod pipeline;

pub use generate::{generate_prd_spec, GenerateResponse};
pub use pipeline::{start_pipeline, PipelineExecutionConfig, StartPipelineResponse};

// The stream module is SSR-only (no client stubs needed)
#[cfg(feature = "ssr")]
pub mod stream;
