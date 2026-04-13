#[cfg(feature = "aws")]
pub mod aws;
#[cfg(feature = "azure")]
pub mod azure;
pub mod chain;
pub mod env;
pub mod file;
#[cfg(feature = "k8s")]
pub mod k8s;
pub mod traits;

pub use chain::ChainSecretProvider;
pub use env::EnvSecretProvider;
pub use file::FileSecretProvider;
pub use traits::{NoopSecretProvider, SecretProvider};
