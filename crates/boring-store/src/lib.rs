pub mod local;
#[cfg(feature = "s3")]
pub mod s3;
pub mod traits;

pub use local::LocalStore;
#[cfg(feature = "s3")]
pub use s3::S3Store;
pub use traits::Store;
