pub mod traits;
pub mod env;
pub mod file;
#[cfg(feature = "k8s")]
pub mod k8s;
#[cfg(feature = "aws")]
pub mod aws;
#[cfg(feature = "azure")]
pub mod azure;
pub mod chain;

pub use traits::{SecretProvider, NoopSecretProvider};
pub use env::EnvSecretProvider;
pub use file::FileSecretProvider;
pub use chain::ChainSecretProvider;
