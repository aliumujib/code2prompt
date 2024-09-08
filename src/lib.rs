pub mod filter;
pub mod git;
pub mod path;
pub mod template;
pub mod token;
use std::error::Error;
use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::json;
pub use filter::should_include_file;
pub use git::{get_git_diff, get_git_diff_between_branches, get_git_log};
pub use path::{label, traverse_directory};
pub use template::{
    copy_to_clipboard, handle_undefined_variables, handlebars_setup, render_template, write_to_file,
};
pub use token::{count_tokens, get_model_info, get_tokenizer};


#[derive(Debug)]
pub struct Code2PromptConfig {
    pub path: std::path::PathBuf,
    pub include: Option<String>,
    pub exclude: Option<String>,
    pub include_priority: bool,
    pub exclude_from_tree: bool,
    pub tokens: bool,
    pub encoding: Option<String>,
    pub output: Option<String>,
    pub diff: bool,
    pub git_diff_branch: Option<String>,
    pub git_log_branch: Option<String>,
    pub line_number: bool,
    pub no_codeblock: bool,
    pub relative_paths: bool,
    pub no_clipboard: bool,
    pub template: Option<std::path::PathBuf>,
    pub json: bool,
}

pub fn generate_prompt(config: &Code2PromptConfig) -> Result<String> {
    // Handlebars Template Setup
    let (template_content, template_name) = get_template(config)?;
    let handlebars = handlebars_setup(&template_content, template_name)?;

    // Parse Patterns
    let include_patterns = parse_patterns(&config.include);
    let exclude_patterns = parse_patterns(&config.exclude);

    // Traverse the directory
    let (tree, files) = traverse_directory(
        &config.path,
        &include_patterns,
        &exclude_patterns,
        config.include_priority,
        config.line_number,
        config.relative_paths,
        config.exclude_from_tree,
        config.no_codeblock,
    )?;

    // Git Diff
    let git_diff = if config.diff {
        get_git_diff(&config.path).unwrap_or_default()
    } else {
        String::new()
    };

    // Git diff between branches
    let git_diff_branch = if let Some(branches) = &config.git_diff_branch {
        let branches = parse_patterns(&Some(branches.to_string()));
        if branches.len() != 2 {
            return Err(anyhow::anyhow!("Please provide exactly two branches separated by a comma."));
        }
        get_git_diff_between_branches(&config.path, &branches[0], &branches[1]).unwrap_or_default()
    } else {
        String::new()
    };

    // Git log between branches
    let git_log_branch = if let Some(branches) = &config.git_log_branch {
        let branches = parse_patterns(&Some(branches.to_string()));
        if branches.len() != 2 {
            return Err(anyhow::anyhow!("Please provide exactly two branches separated by a comma."));
        }
        get_git_log(&config.path, &branches[0], &branches[1]).unwrap_or_default()
    } else {
        String::new()
    };

    // Prepare JSON Data
    let mut data = json!({
        "absolute_code_path": label(&config.path),
        "source_tree": tree,
        "files": files,
        "git_diff": git_diff,
        "git_diff_branch": git_diff_branch,
        "git_log_branch": git_log_branch
    });

    // Handle undefined variables
    handle_undefined_variables(&mut data, &template_content)?;

    // Render the template
    let rendered = render_template(&handlebars, template_name, &data)?;

    // Handle token count if requested
    if config.tokens {
        let bpe = get_tokenizer(&config.encoding);
        let token_count = bpe.encode_with_special_tokens(&rendered).len();
        let model_info = get_model_info(&config.encoding);
        println!(
            "{}{}{} Token count: {}, Model info: {}",
            "[".bold().white(),
            "i".bold().blue(),
            "]".bold().white(),
            token_count.to_string().bold().yellow(),
            model_info
        );
    }

    // Handle JSON output if requested
    if config.json {
        let json_output = json!({
            "prompt": rendered,
            "directory_name": label(&config.path),
            "token_count": get_tokenizer(&config.encoding).encode_with_special_tokens(&rendered).len(),
            "model_info": get_model_info(&config.encoding),
            "files": files.iter().filter_map(|file| file.get("path").and_then(|p| p.as_str()).map(|s| s.to_string())).collect::<Vec<String>>(),
        });
        return Ok(serde_json::to_string_pretty(&json_output)?);
    }

    // Handle clipboard copy if not disabled
    if !config.no_clipboard {
        if let Err(e) = copy_to_clipboard(&rendered) {
            eprintln!(
                "{}{}{} {}",
                "[".bold().white(),
                "!".bold().red(),
                "]".bold().white(),
                format!("Failed to copy to clipboard: {}", e).red()
            );
        } else {
            println!(
                "{}{}{} {}",
                "[".bold().white(),
                "✓".bold().green(),
                "]".bold().white(),
                "Copied to clipboard successfully.".green()
            );
        }
    }

    // Handle output file if specified
    if let Some(output_path) = &config.output {
        write_to_file(output_path, &rendered)?;
    }

    Ok(rendered)
}

fn get_template(config: &Code2PromptConfig) -> Result<(String, &'static str)> {
    if let Some(template_path) = &config.template {
        let content = std::fs::read_to_string(template_path)
            .context("Failed to read custom template file")?;
        Ok((content, "custom"))
    } else {
        Ok((include_str!("default_template.hbs").to_string(), "default"))
    }
}

fn parse_patterns(patterns: &Option<String>) -> Vec<String> {
    match patterns {
        Some(patterns) if !patterns.is_empty() => {
            patterns.split(',').map(|s| s.trim().to_string()).collect()
        }
        _ => vec![],
    }
}