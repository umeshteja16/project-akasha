//! Grounded chat (ADR 0012): answer questions from the user's own files.
//!
//! A question is searched with `akasha_search::search_chunks` (owner-scoped,
//! optionally limited to some files), the refusal gate ([`evidence`]) checks the
//! best passage, the top passages become numbered sources in the prompt
//! ([`prompt`]), the model's answer streams back as Server-Sent Events
//! ([`events`]) and is stored with the sources it cites ([`citations`]).

pub mod citations;
pub mod events;
pub mod evidence;
mod generate;
pub mod prompt;
pub mod turn;
