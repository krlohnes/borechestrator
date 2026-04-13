pub mod docker;
#[cfg(feature = "k8s")]
pub mod k8s;
pub mod local;
pub mod traits;

pub use docker::DockerRuntime;
pub use local::LocalRuntime;
pub use traits::{JobHandle, JobSpec, JobStatus, Runtime};
