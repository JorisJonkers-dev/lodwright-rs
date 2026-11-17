// SPDX-License-Identifier: GPL-3.0-only
//! The library API of lodwright: generate LOD in process, with progress, cancel and copyable errors.
//!
//! A caller builds a [`GenerateRequest`] from a `lodwright-request` document, passes a [`Progress`] receiver and a
//! [`CancelToken`], and gets a [`GenerateOutcome`] or an [`Error`]:
//!
//! ```no_run
//! use lodwright_api::{CancelToken, GenerateOutcome, GenerateRequest, ProgressEvent, generate};
//! use lodwright_contract::Request;
//!
//! let request = GenerateRequest::new(Request::new("/data", &["Base.esm"], "/lod-out"));
//! let cancel = CancelToken::new(); // a clone can cancel from another thread
//! let mut progress = |event: &ProgressEvent| eprintln!("{} {:?}/{:?}", event.label_key(), event.done, event.total);
//! match generate(&request, &mut progress, &cancel) {
//!     Ok(GenerateOutcome::Done { result }) => println!("{}", result.code),
//!     Ok(GenerateOutcome::Cancelled) => println!("stopped; the next run resumes"),
//!     Err(e) => println!("{}", e.copy_text()),
//! }
//! ```
//!
//! The library prints nothing and never ends the process; the `lodwright` command line is a wrapper over it.
//! [`API_VERSION`] is the version of this interface; `lodwright --version --json` lists it as the `lodwright-api`
//! contract.

mod error;
mod generate;
pub mod load;
mod progress;

pub use error::Error;
pub use generate::{GenerateOutcome, GenerateRequest, generate};
pub use lodwright_contract::{API_CONTRACT, API_VERSION};
pub use progress::{CancelToken, NoProgress, Progress, ProgressEvent, Step};
