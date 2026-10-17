//! Shows exactly what the writer model would receive for a dictation: the transcript goes
//! through Kudu's rules, then the routing decision and the full prompt are printed. Turns real
//! dictations into test cases for the kudu-models repository. Needs no model.
//!
//! cargo run -p kudu-engine --no-default-features --example writer_prompt -- [--category email|chat|work|code] [--app Slack] [--long] "transcript"

use kudu_core::{formatting_need, AppCategory, CleanupSettings, EditingStrength, Placeholders, TextContext, TextPipeline};
use kudu_engine::prompts::{system_prompt, user_prompt, FormatRequest};

fn main() {
    let mut category = AppCategory::General;
    let mut app_name = None;
    let mut short = true;
    let mut transcript = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--category" => {
                category = match args.next().as_deref() {
                    Some("email") => AppCategory::Email,
                    Some("chat") => AppCategory::PersonalMessage,
                    Some("work") => AppCategory::WorkMessage,
                    Some("code") => AppCategory::Code,
                    _ => AppCategory::General,
                }
            }
            "--app" => app_name = args.next(),
            "--long" => short = false,
            _ => transcript.push(arg),
        }
    }
    let transcript = transcript.join(" ");
    if transcript.is_empty() {
        eprintln!("usage: writer_prompt [--category email|chat|work|code] [--app NAME] [--long] \"transcript\"");
        std::process::exit(2);
    }

    let cleanup = CleanupSettings::default();
    let style = cleanup.styles.style_for(category);
    let rules = TextPipeline::before_formatting(&cleanup, &[], &[], Placeholders::default());
    let text = rules.process(&transcript, &TextContext::new(category, None, style));
    let strength = EditingStrength::Light;
    let routed = if category == AppCategory::Code {
        formatting_need::should_format_code(&text, strength)
    } else {
        formatting_need::should_format(&text, strength, category)
    };
    let request = FormatRequest { text, category, style, strength, app_name, ..Default::default() };

    println!("routed to the writer: {}", if routed { "yes" } else { "no (the rules finish it)" });
    println!("\n--- system ---\n{}", system_prompt(category, short));
    println!("\n--- user ---\n{}", user_prompt(&request));
}
