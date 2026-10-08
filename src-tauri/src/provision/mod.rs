pub mod apk;
pub mod executor;
pub mod plan;
pub mod recipe;
pub mod types;

pub const DEFAULT_MINIMUM_FREE_GIB: u32 = 2;
pub const GIB_BYTES: u64 = 1024 * 1024 * 1024;
pub const MIN_FREE_BYTES: u64 = DEFAULT_MINIMUM_FREE_GIB as u64 * GIB_BYTES;

pub use executor::AndroidProvisionExecutor;
pub use plan::{
    build_plan, run_plan, ProvisionCheck, ProvisionProgress, ProvisionProgressStore,
    ProvisionRunOutcome, ProvisionStepExecutor, ProvisionStepRunOutcome,
};
pub use recipe::{bundled_recipes, ProvisionRecipe, ProvisionTermuxSource};
pub use types::{
    ProvisionConsent, ProvisionEvent, ProvisionPlan, ProvisionStep, ProvisionStepId,
    ProvisionStepState,
};
