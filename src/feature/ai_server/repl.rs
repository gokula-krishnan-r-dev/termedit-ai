//! Interactive REPL for `--ai-server`.

use anyhow::Result;
use std::io::{BufRead, Write};

use super::apply;
use super::gemini::LlmClient;
use super::prompt::{system_instruction_json_only, user_message};
use super::response::AssistantPlan;
use super::ui;
use super::{collect_context, AiServerOptions};

pub struct ReplOptions {
    pub dry_run: bool,
}

pub async fn run_repl(llm: &dyn LlmClient, opts: &AiServerOptions) -> Result<()> {
    println!("TermEdit AI server context mode — type a question, or:");
    println!("  /refresh  Rebuild context (ignore cache)");
    println!("  /quit     Exit");
    let stdin = std::io::stdin();
    let mut session_ctx: Option<super::context::ServerContext> = None;
    loop {
        print!("\nai-server> ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "/quit" || line == "/exit" {
            break;
        }
        if line == "/refresh" {
            session_ctx = Some(collect_context(opts, true).await?);
            println!("Context refreshed ({} notes).", session_ctx.as_ref().unwrap().notes.len());
            continue;
        }
        let ctx = if let Some(ref c) = session_ctx {
            c.clone()
        } else {
            let c = collect_context(opts, false).await?;
            session_ctx = Some(c.clone());
            c
        };
        let sys = system_instruction_json_only();
        let user = user_message(line, &ctx)?;
        let raw = llm.generate_json(&sys, &user).await?;
        let plan = AssistantPlan::parse_model_text(&raw)?;
        print_plan(&plan);
        apply::offer_apply_plan(&plan, opts.dry_run)?;
    }
    Ok(())
}

/// One-shot query (non-interactive).
pub async fn run_once(llm: &dyn LlmClient, query: &str, opts: &AiServerOptions) -> Result<()> {
    let ctx = collect_context(opts, false).await?;
    let sys = system_instruction_json_only();
    let user = user_message(query, &ctx)?;
    let raw = llm.generate_json(&sys, &user).await?;
    let plan = AssistantPlan::parse_model_text(&raw)?;
    print_plan(&plan);
    apply::offer_apply_plan(&plan, opts.dry_run)?;
    Ok(())
}

fn print_plan(plan: &AssistantPlan) {
    ui::print_section("Explanation");
    println!("{}", plan.explanation);
    if !plan.suggested_fixes.is_empty() {
        ui::print_section("Suggested fixes");
        for (i, s) in plan.suggested_fixes.iter().enumerate() {
            println!("  {}. {}", i + 1, s);
        }
    }
    if !plan.shell_commands.is_empty() {
        ui::print_section("Shell commands (review before running)");
        for c in &plan.shell_commands {
            println!("  $ {}", c);
        }
    }
}
