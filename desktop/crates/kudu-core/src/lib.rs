//! Kudu's platform-independent core for Windows and Linux: the text pipeline that turns a raw
//! transcript into finished text, customizable shortcuts, and learning from corrections.
//! A port of the macOS app's `KuduCore` (Swift), kept behaviour-for-behaviour identical.

pub mod app_category;
pub mod audio_levels;
pub mod calendar;
pub mod cleanup_settings;
pub mod command_output;
pub mod correction_learner;
pub mod editing_strength;
pub mod formatting_guard;
pub mod formatting_need;
pub mod id;
pub mod insights;
pub mod pipeline;
pub mod sentence_chunker;
pub mod shortcut;
pub mod stages;
pub mod string_casing;
pub mod term_extractor;
pub mod term_speller;
pub mod usage_stats;
pub mod writing_style;

mod text;

pub use app_category::{category_for, is_terminal, AppCategory};
pub use calendar::{Calendar, Day};
pub use cleanup_settings::CleanupSettings;
pub use correction_learner::LearnedCorrection;
pub use editing_strength::EditingStrength;
pub use insights::{DayTotal, Insights};
pub use pipeline::{TextContext, TextPipeline, TextStage};
pub use sentence_chunker::Chunks;
pub use stages::{DictionaryEntry, DictionaryStage, Placeholders, Snippet, SnippetStage};
pub use term_speller::TermSpeller;
pub use usage_stats::{word_count, DictationRecord, UsageStats};
pub use writing_style::{StyleSettings, StyleStage, WritingStyle};
