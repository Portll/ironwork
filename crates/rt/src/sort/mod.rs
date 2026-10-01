//! SORT and MERGE. [`Keys`] orders plain records under keys described as data, as DFSORT orders
//! them, for any caller; the rest runs a program's SORT, MERGE, RELEASE and RETURN and its table
//! SORTs over a [`SortHost`], with the keys as the program's items describe them.

mod keys;
mod run;

pub use keys::{Collating, Format, Key, KeyError, KeyValue, Keys, decimal, float_order, order};
pub use run::{Active, ItemKey, Procedure, SortFile, SortHost, item_keys, key_values, release, release_ready, return_record, sort, sort_table};
