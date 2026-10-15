//! Generic storage calls generated from standard-library declarations.
use crate::{Type, Value};
mod call;
pub(super) use call::Call;
mod output;
pub(super) use output::ResultShape;
mod query;
mod services;
pub(super) use query::Query;
mod iteration;
pub(super) use iteration::{with_borrowed_sequence, with_owned_sequence};
mod construction;
pub(super) use construction::{construct_array, construct_empty};
