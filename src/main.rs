mod analyzer;
mod cli;
mod output;

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use tracing::debug;
use tracing_subscriber::EnvFilter;

use crate::analyzer::process_directory;
use crate::cli::{Cli, CodeArgs, Commands, FunctionTarget, IndexCommand};
use glimpse::code::extract::Extractor;
use glimpse::code::graph::CallGraph;
use glimpse::code::index::{
    clear_index, file_fingerprint, load_index, save_index, FileRecord, Index,
};
use glimpse::code::lsp::AsyncLspResolver;
use glimpse::fetch::{GitProcessor, UrlProcessor};
use glimpse::{
    get_config_path, is_source_file, load_config, load_repo_config, save_config, save_repo_config,
    RepoConfig,
};

fn is_url_or_git(path: &str) -> bool {
    GitProcessor::is_git_url(path) || path.starts_with("http://") || path.starts_with("https://")
}

fn has_custom_options(args: &Cli) -> bool {
    args.include.is_some()
        || args.exclude.is_some()
        || args.max_size.is_some()
        || args.max_depth.is_some()
        || args.output.is_some()
        || args.file.is_some()
        || args.hidden
        || args.no_ignore
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .without_time()
        .init();

    let mut config = load_config()?;
    let mut args = Cli::parse_with_config(&config)?;

    debug!("config loaded, args parsed");

    if let Some(ref cmd) = args.command {
        return match cmd {
            Commands::Code(code_args) => handle_code_command(code_args),
            Commands::Index(index_args) => handle_index_command(&index_args.command),
        };
    }

    if args.config_path {
        let path = get_config_path()?;
        println!("{}", path.display());
        return Ok(());
    }

    let url_paths: Vec<_> = args
        .paths
        .iter()
        .filter(|path| is_url_or_git(path))
        .take(1)
        .cloned()
        .collect();

    if url_paths.is_empty() && !args.paths.is_empty() {
        let base_path = PathBuf::from(&args.paths[0]);
        let root_dir = find_containing_dir_with_glimpse(&base_path)?;
        let glimpse_file = root_dir.join(".glimpse");

        if args.config {
            let repo_config = create_repo_config_from_args(&args);
            save_repo_config(&glimpse_file, &repo_config)?;
            println!("Configuration saved to {}", glimpse_file.display());

            if let Ok(canonical_root) = std::fs::canonicalize(&root_dir) {
                let root_str = canonical_root.to_string_lossy().to_string();
                if let Some(pos) = config
                    .skipped_prompt_repos
                    .iter()
                    .position(|p| p == &root_str)
                {
                    config.skipped_prompt_repos.remove(pos);
                    save_config(&config)?;
                }
            }
        } else if glimpse_file.exists() {
            println!("Loading configuration from {}", glimpse_file.display());
            let repo_config = load_repo_config(&glimpse_file)?;
            apply_repo_config(&mut args, &repo_config);
        } else if has_custom_options(&args) {
            let canonical_root = std::fs::canonicalize(&root_dir).unwrap_or(root_dir.clone());
            let root_str = canonical_root.to_string_lossy().to_string();

            if !config.skipped_prompt_repos.contains(&root_str) {
                print!(
                    "Would you like to save these options as defaults for this directory? (y/n): "
                );
                io::stdout().flush()?;
                let mut response = String::new();
                io::stdin().read_line(&mut response)?;

                if response.trim().to_lowercase() == "y" {
                    let repo_config = create_repo_config_from_args(&args);
                    save_repo_config(&glimpse_file, &repo_config)?;
                    println!("Configuration saved to {}", glimpse_file.display());

                    if let Some(pos) = config
                        .skipped_prompt_repos
                        .iter()
                        .position(|p| p == &root_str)
                    {
                        config.skipped_prompt_repos.remove(pos);
                        save_config(&config)?;
                    }
                } else {
                    config.skipped_prompt_repos.push(root_str);
                    save_config(&config)?;
                }
            }
        }
    }

    if url_paths.len() > 1 {
        return Err(anyhow::anyhow!(
            "Only one URL or git repository can be processed at a time"
        ));
    }

    if let Some(url_path) = url_paths.first() {
        if GitProcessor::is_git_url(url_path) {
            let git_processor = GitProcessor::new()?;
            let repo_path = git_processor.process_repo(url_path)?;
            args.validate_args(true)?;

            let mut subpaths: Vec<String> = vec![];
            let mut found_url = false;
            for p in &args.paths {
                if !found_url && p.as_str() == url_path.as_str() {
                    found_url = true;
                    continue;
                }
                if found_url {
                    subpaths.push(p.clone());
                }
            }

            let process_args = if subpaths.is_empty() {
                args.with_path(repo_path.to_str().unwrap())
            } else {
                let mut new_args = args.clone();
                new_args.paths = subpaths
                    .iter()
                    .map(|sub| {
                        let mut joined = std::path::PathBuf::from(&repo_path);
                        joined.push(sub);
                        joined.to_string_lossy().to_string()
                    })
                    .collect();
                new_args
            };
            process_directory(&process_args)?;
        } else if url_path.starts_with("http://") || url_path.starts_with("https://") {
            args.validate_args(true)?;
            let link_depth = args.link_depth.unwrap_or(config.default_link_depth);
            let traverse = args.traverse_links || config.traverse_links;

            let mut processor = UrlProcessor::new(link_depth);
            let content = processor.process_url(url_path, traverse)?;

            if let Some(output_file) = &args.file {
                fs::write(output_file, content)?;
                println!("Output written to: {}", output_file.display());
            } else if args.print {
                println!("{content}");
            } else {
                match arboard::Clipboard::new()
                    .and_then(|mut clipboard| clipboard.set_text(content))
                {
                    Ok(_) => println!("URL content copied to clipboard"),
                    Err(_) => {
                        println!("Failed to copy to clipboard, use -f to save to a file instead")
                    }
                }
            }
        }
    } else {
        args.validate_args(false)?;
        process_directory(&args)?;
    }

    Ok(())
}

fn find_containing_dir_with_glimpse(path: &Path) -> anyhow::Result<PathBuf> {
    let mut current = if path.is_file() {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        path.to_path_buf()
    };

    loop {
        if current.join(".glimpse").exists() {
            return Ok(current);
        }

        if !current.pop() {
            return Ok(if path.is_file() {
                path.parent().unwrap_or(Path::new(".")).to_path_buf()
            } else {
                path.to_path_buf()
            });
        }
    }
}

fn create_repo_config_from_args(args: &Cli) -> RepoConfig {
    RepoConfig {
        include: args.include.clone(),
        exclude: args.exclude.clone(),
        max_size: args.max_size,
        max_depth: args.max_depth,
        output: args.get_output_format(),
        file: args.file.clone(),
        hidden: Some(args.hidden),
        no_ignore: Some(args.no_ignore),
    }
}

fn apply_repo_config(args: &mut Cli, repo_config: &RepoConfig) {
    if let Some(ref include) = repo_config.include {
        args.include = Some(include.clone());
    }

    if let Some(ref exclude) = repo_config.exclude {
        args.exclude = Some(exclude.clone());
    }

    if let Some(max_size) = repo_config.max_size {
        args.max_size = Some(max_size);
    }

    if let Some(max_depth) = repo_config.max_depth {
        args.max_depth = Some(max_depth);
    }

    if let Some(ref output) = repo_config.output {
        args.output = Some(output.clone().into());
    }

    if let Some(ref file) = repo_config.file {
        args.file = Some(file.clone());
    }

    if let Some(hidden) = repo_config.hidden {
        args.hidden = hidden;
    }

    if let Some(no_ignore) = repo_config.no_ignore {
        args.no_ignore = no_ignore;
    }
}

fn handle_code_command(args: &CodeArgs) -> Result<()> {
    let root = args
        .root
        .canonicalize()
        .unwrap_or_else(|_| args.root.clone());
    let target = FunctionTarget::parse(&args.target)?;

    let mut index = load_index(&root)?.unwrap_or_else(Index::new);
    let needs_update = index_directory(&root, &mut index, args.hidden, args.no_ignore)?;
    let mut needs_save = needs_update > 0;

    // Only run LSP resolution if:
    // 1. --precise is requested
    // 2. Either files were updated OR no calls have been resolved yet (first --precise run)
    let has_any_resolved = index.calls().any(|c| c.resolved.is_some());
    if args.precise && (needs_update > 0 || !has_any_resolved) {
        let resolved = resolve_calls_with_lsp(&root, &mut index)?;
        if resolved > 0 {
            needs_save = true;
        }
    }

    if needs_save {
        save_index(&index, &root)?;
    }

    // After LSP resolution, use build_with_options which checks call.resolved first
    // This avoids creating another LSP resolver and re-trying failed calls
    let graph = CallGraph::build_with_options(&index, args.strict);

    let node_id = if let Some(ref file) = target.file {
        let file_path = root.join(file);
        let rel_path = file_path
            .strip_prefix(&root)
            .unwrap_or(&file_path)
            .to_path_buf();
        graph
            .find_node_by_file_and_name(&rel_path, &target.function)
            .or_else(|| graph.find_node_by_file_and_name(&file_path, &target.function))
    } else {
        graph.find_node(&target.function)
    };

    let Some(node_id) = node_id else {
        bail!("function '{}' not found in index", target.function);
    };

    let depth = args.depth.unwrap_or(1);

    let definitions = if args.callers {
        graph
            .get_callers_to_depth(node_id, depth)
            .into_iter()
            .filter_map(|id| graph.get_node(id).map(|n| &n.definition))
            .collect()
    } else {
        graph.definitions_to_depth(node_id, depth)
    };

    let output = format_definitions(&definitions, &root)?;

    if let Some(ref file) = args.file {
        fs::write(file, &output)?;
        eprintln!("Output written to: {}", file.display());
    } else {
        print!("{}", output);
    }

    Ok(())
}

fn handle_index_command(cmd: &IndexCommand) -> Result<()> {
    match cmd {
        IndexCommand::Build {
            path,
            force,
            precise,
            hidden,
            no_ignore,
        } => {
            let root = path.canonicalize().unwrap_or_else(|_| path.clone());

            let mut index = if *force {
                Index::new()
            } else {
                load_index(&root)?.unwrap_or_else(Index::new)
            };

            let updated = index_directory(&root, &mut index, *hidden, *no_ignore)?;

            // Only run LSP resolution if files were updated or no calls resolved yet
            let has_any_resolved = index.calls().any(|c| c.resolved.is_some());
            if *precise && (updated > 0 || !has_any_resolved) {
                let resolved = resolve_calls_with_lsp(&root, &mut index)?;
                if resolved > 0 {
                    eprintln!("Resolved {} calls with LSP", resolved);
                }
            }

            save_index(&index, &root)?;

            let file_count = index.files.len();
            let def_count = index.definitions().count();
            let call_count = index.calls().count();
            let resolved_count = index.calls().filter(|c| c.resolved.is_some()).count();

            if updated > 0 || *precise {
                eprintln!(
                    "Index updated: {} files ({} updated), {} definitions, {} calls ({} resolved)",
                    file_count, updated, def_count, call_count, resolved_count
                );
            } else {
                eprintln!(
                    "Index up to date: {} files, {} definitions, {} calls ({} resolved)",
                    file_count, def_count, call_count, resolved_count
                );
            }
        }
        IndexCommand::Clear { path } => {
            let root = path.canonicalize().unwrap_or_else(|_| path.clone());
            clear_index(&root)?;
            eprintln!("Index cleared for: {}", root.display());
        }
        IndexCommand::Status { path } => {
            let root = path.canonicalize().unwrap_or_else(|_| path.clone());

            match load_index(&root)? {
                Some(index) => {
                    let file_count = index.files.len();
                    let def_count = index.definitions().count();
                    let call_count = index.calls().count();
                    let import_count = index.imports().count();

                    println!("Index status for: {}", root.display());
                    println!("  Files:       {}", file_count);
                    println!("  Definitions: {}", def_count);
                    println!("  Calls:       {}", call_count);
                    println!("  Imports:     {}", import_count);
                }
                None => {
                    println!("No index found for: {}", root.display());
                }
            }
        }
    }

    Ok(())
}

const INDEX_CHUNK_SIZE: usize = 256;

fn index_directory(root: &Path, index: &mut Index, hidden: bool, no_ignore: bool) -> Result<usize> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .expect("valid template"),
    );
    pb.set_message("scanning files...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let source_files: Vec<_> = ignore::WalkBuilder::new(root)
        .hidden(!hidden)
        .git_ignore(!no_ignore)
        .ignore(!no_ignore)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter(|e| is_source_file(e.path()))
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| !ext.is_empty())
        })
        .collect();

    pb.set_message(format!(
        "found {} source files, checking for changes...",
        source_files.len()
    ));

    let stale_files: Vec<_> = source_files
        .into_iter()
        .filter_map(|entry| {
            let path = entry.path();
            let rel_path = path.strip_prefix(root).unwrap_or(path);
            let ext = path.extension().and_then(|e| e.to_str())?;
            if ext.is_empty() {
                return None;
            }
            let (mtime, size) = file_fingerprint(path).ok()?;
            if index.is_stale(rel_path, mtime, size) {
                Some((
                    path.to_path_buf(),
                    rel_path.to_path_buf(),
                    ext.to_string(),
                    mtime,
                    size,
                ))
            } else {
                None
            }
        })
        .collect();

    pb.finish_and_clear();

    let total = stale_files.len();
    if total == 0 {
        return Ok(0);
    }

    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .expect("valid template")
            .progress_chars("#>-"),
    );
    pb.set_message("indexing...");

    let mut updated = 0;

    for chunk in stale_files.chunks(INDEX_CHUNK_SIZE) {
        let records: Vec<FileRecord> = chunk
            .par_iter()
            .filter_map(|(path, rel_path, ext, mtime, size)| {
                let extractor = match Extractor::from_extension(ext) {
                    Ok(e) => e,
                    Err(e) => {
                        debug!(ext = %ext, error = ?e, "no extractor for extension");
                        return None;
                    }
                };
                let source = fs::read(path).ok()?;

                let mut parser = tree_sitter::Parser::new();
                parser.set_language(extractor.language()).ok()?;
                let tree = parser.parse(&source, None)?;

                let definitions = extractor.extract_definitions(&tree, &source, rel_path);
                let calls = extractor.extract_calls(&tree, &source, rel_path);
                let imports = extractor.extract_imports(&tree, &source, rel_path);

                pb.inc(1);

                Some(FileRecord {
                    path: rel_path.to_path_buf(),
                    mtime: *mtime,
                    size: *size,
                    definitions,
                    calls,
                    imports,
                })
            })
            .collect();

        updated += records.len();
        for record in records {
            index.update(record);
        }
    }

    pb.finish_and_clear();
    Ok(updated)
}

type CacheKey = (String, Option<String>, PathBuf);

fn resolve_calls_with_lsp(root: &Path, index: &mut Index) -> Result<usize> {
    use glimpse::code::index::ResolvedCall;

    let unresolved_count: usize = index
        .files
        .values()
        .map(|r| r.calls.iter().filter(|c| c.resolved.is_none()).count())
        .sum();

    if unresolved_count == 0 {
        return Ok(0);
    }

    let pb = ProgressBar::new(unresolved_count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .expect("valid template")
            .progress_chars("#>-"),
    );
    pb.set_message("resolving calls with LSP...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let rt = tokio::runtime::Runtime::new()?;
    let concurrency = 50;

    let (resolved, stats, cache_hits, cache_misses) = rt.block_on(async {
        let mut resolver = AsyncLspResolver::new(root);
        let mut cache: HashMap<CacheKey, Option<ResolvedCall>> = HashMap::new();
        let mut cache_hits = 0usize;
        let mut cache_misses = 0usize;
        let mut total_resolved = 0usize;

        let file_paths: Vec<_> = index.files.keys().cloned().collect();

        for file_path in &file_paths {
            let calls_to_resolve: Vec<_> = {
                let Some(record) = index.files.get(file_path) else {
                    continue;
                };
                record
                    .calls
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.resolved.is_none())
                    .map(|(i, c)| (i, c.clone()))
                    .collect()
            };

            if calls_to_resolve.is_empty() {
                continue;
            }

            let call_count = calls_to_resolve.len();
            let mut calls_for_lsp = Vec::new();
            let mut cached_resolutions = Vec::new();

            for (call_idx, call) in &calls_to_resolve {
                let cache_key: CacheKey = (
                    call.callee.clone(),
                    call.qualifier.clone(),
                    call.file.clone(),
                );

                if let Some(cached_result) = cache.get(&cache_key) {
                    cache_hits += 1;
                    if let Some(resolved_call) = cached_result.clone() {
                        cached_resolutions.push((*call_idx, resolved_call));
                    }
                } else {
                    cache_misses += 1;
                    calls_for_lsp.push((*call_idx, call.clone(), cache_key));
                }
            }

            if !calls_for_lsp.is_empty() {
                let call_refs: Vec<_> = calls_for_lsp.iter().map(|(_, c, _)| c).collect();
                let results = resolver
                    .resolve_calls_batch(&call_refs, index, concurrency)
                    .await;

                for (batch_idx, resolved_call) in results {
                    let (call_idx, _, cache_key) = &calls_for_lsp[batch_idx];
                    cache.insert(cache_key.clone(), Some(resolved_call.clone()));

                    if let Some(record) = index.files.get_mut(file_path) {
                        if *call_idx < record.calls.len() {
                            record.calls[*call_idx].resolved = Some(resolved_call);
                            total_resolved += 1;
                        }
                    }
                }

                for (_, _, cache_key) in &calls_for_lsp {
                    if !cache.contains_key(cache_key) {
                        cache.insert(cache_key.clone(), None);
                    }
                }
            }

            for (call_idx, resolved_call) in cached_resolutions {
                if let Some(record) = index.files.get_mut(file_path) {
                    if call_idx < record.calls.len() {
                        record.calls[call_idx].resolved = Some(resolved_call);
                        total_resolved += 1;
                    }
                }
            }

            pb.inc(call_count as u64);
        }

        resolver.shutdown_all().await;
        let stats = resolver.stats().clone();
        (total_resolved, stats, cache_hits, cache_misses)
    });

    pb.finish_and_clear();

    if stats.by_server.is_empty() {
        debug!("LSP: no servers responded");
    } else {
        debug!("LSP: {}", stats);
    }

    let total_lookups = cache_hits + cache_misses;
    if total_lookups > 0 {
        let hit_rate = (cache_hits as f64 / total_lookups as f64) * 100.0;
        debug!(
            cache_hits,
            total_lookups,
            hit_rate = format!("{:.1}%", hit_rate),
            unique_lookups = cache_misses,
            "resolution cache stats"
        );
    }

    Ok(resolved)
}

fn format_definitions(
    definitions: &[&glimpse::code::index::Definition],
    root: &Path,
) -> Result<String> {
    use std::fmt::Write;

    let mut output = String::new();

    for def in definitions {
        let file_path = root.join(&def.file);
        let content = fs::read_to_string(&file_path)
            .with_context(|| format!("failed to read: {}", file_path.display()))?;

        let lines: Vec<&str> = content.lines().collect();
        let start = def.span.start_line.saturating_sub(1);
        let end = def.span.end_line.min(lines.len());

        writeln!(output, "## {}:{}", def.file.display(), def.name)?;
        writeln!(output)?;
        writeln!(output, "```")?;
        for line in &lines[start..end] {
            writeln!(output, "{}", line)?;
        }
        writeln!(output, "```")?;
        writeln!(output)?;
    }

    Ok(output)
}
