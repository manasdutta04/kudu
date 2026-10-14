//! The individual cleanup stages of the text pipeline.

mod cursor_fitter;
mod dictionary;
mod file_name;
mod filler;
mod line_start;
mod mention;
mod message_layout;
mod self_correction;
mod snippet;
mod spoken_commands;
mod time_normalizer;
mod whitespace;

pub use cursor_fitter::CursorFitter;
pub use dictionary::{DictionaryEntry, DictionaryStage};
pub use file_name::FileNameStage;
pub use filler::{FillerRemover, DEFAULT_FILLERS};
pub use line_start::LineStartCapitalizer;
pub use mention::MentionStage;
pub use message_layout::MessageLayout;
pub use self_correction::SelfCorrection;
pub use snippet::{ClipboardFn, NowFn, Placeholders, Snippet, SnippetStage};
pub use spoken_commands::SpokenCommands;
pub use time_normalizer::TimeNormalizer;
pub use whitespace::WhitespaceNormalizer;
