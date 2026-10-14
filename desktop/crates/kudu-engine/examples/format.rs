//! Tries smart formatting and Command mode with a downloaded GGUF model.
//!
//! cargo run --release -p kudu-engine --features smart --example format -- model.gguf

use std::time::Instant;

use kudu_core::{AppCategory, EditingStrength};
use kudu_engine::smart::{FormatRequest, SmartFormatter};

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("path to a .gguf model");
    let formatter = SmartFormatter::load(path.as_ref())?;
    let cases: [(&str, AppCategory, Option<&str>); 6] = [
        ("quick update three things first design is done second backend is waiting on auth third should be ready tomorrow", AppCategory::General, Some("Notepad")),
        ("So for the launch next week we need three things first the landing page needs a final review second the pricing table has to be updated with the new plans and third we have to send the announcement email to everyone on the waitlist", AppCategory::General, None),
        ("add two kilo atta and the call is at eight pm which costs twelve thousand dollars", AppCategory::General, None),
        ("hey can you review my pull request before lunch thanks", AppCategory::WorkMessage, Some("Slack")),
        ("git commit dash m fixed the auth retry loop", AppCategory::Code, Some("Windows Terminal")),
        ("Dear Priya thanks for sending over the contract yesterday I have gone through it carefully overall everything looks good could you send an updated version by Friday best regards Manas", AppCategory::Email, Some("Outlook")),
    ];
    for (text, category, app) in cases {
        let start = Instant::now();
        let request = FormatRequest {
            text: text.into(),
            category,
            strength: EditingStrength::Light,
            app_name: app.map(str::to_string),
            ..Default::default()
        };
        let result = formatter.format(&request);
        println!("• {text}\n  → {}   [{} ms]\n", result.as_deref().unwrap_or("(kept as is)"), start.elapsed().as_millis());
    }
    for (instruction, selection) in [
        ("make this more formal", Some("hey can't make it today, can we do tomorrow?")),
        ("translate to Hindi", Some("See you tomorrow at the office.")),
        ("fix the grammar", Some("me and him goes to the market yesterday")),
        ("write a polite reply saying I'll be late by ten minutes", None),
    ] {
        let start = Instant::now();
        let reply = formatter.command(instruction, selection, AppCategory::Email);
        println!("Command \"{instruction}\": {reply:?}   [{} ms]", start.elapsed().as_millis());
    }
    Ok(())
}
