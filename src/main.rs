mod analyzer;
mod cli;
mod output;
mod progress;

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rayon::prelude::*;
use tracing::debug;
use tracing_subscriber::EnvFilter;

use crate::analyzer::process_directory;
use crate::cli::{Cli, CodeArgs, Commands, FunctionTarget, IndexCommand};
use crate::progress::ProgressContext;
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
    let mut progress = ProgressContext::new();

    // Scan for stale files
    progress.scanning();
    let source_files: Vec<_> = ignore::WalkBuilder::new(&root)
        .hidden(!args.hidden)
        .git_ignore(!args.no_ignore)
        .ignore(!args.no_ignore)
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

    let stale_files: Vec<_> = source_files
        .into_iter()
        .filter_map(|entry| {
            let path = entry.path();
            let rel_path = path.strip_prefix(&root).unwrap_or(path);
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

    let files_to_index = stale_files.len();
    let unresolved_calls = if args.precise {
        index
            .files
            .values()
            .map(|r| r.calls.iter().filter(|c| c.resolved.is_none()).count())
            .sum::<usize>()
    } else {
        0
    };

    let total_work = files_to_index + unresolved_calls;
    if total_work > 0 {
        progress.set_indexing_total(files_to_index as u64);
        if args.precise {
            progress.set_lsp_total(unresolved_calls as u64);
        }
    }

    // Index stale files
    let mut needs_update = 0;
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

                progress.indexing_file(rel_path);

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

        needs_update += records.len();
        for record in records {
            index.update(record);
        }
    }

    let mut needs_save = needs_update > 0;

    let has_any_resolved = index.calls().any(|c| c.resolved.is_some());
    if args.precise && (needs_update > 0 || !has_any_resolved) {
        let new_unresolved: usize = index
            .files
            .values()
            .map(|r| r.calls.iter().filter(|c| c.resolved.is_none()).count())
            .sum();

        if new_unresolved > 0 {
            progress.set_lsp_total(new_unresolved as u64);
            let resolved = resolve_calls_with_lsp(&root, &mut index, &progress)?;
            if resolved > 0 {
                needs_save = true;
            }
        }
    }
    progress.finish_clear();

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

            let mut progress = ProgressContext::new();

            // First pass: scan to find stale files
            progress.scanning();
            let source_files: Vec<_> = ignore::WalkBuilder::new(&root)
                .hidden(!*hidden)
                .git_ignore(!*no_ignore)
                .ignore(!*no_ignore)
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

            let stale_files: Vec<_> = source_files
                .into_iter()
                .filter_map(|entry| {
                    let path = entry.path();
                    let rel_path = path.strip_prefix(&root).unwrap_or(path);
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

            let files_to_index = stale_files.len() as u64;

            // Count unresolved calls for LSP phase
            let unresolved_calls = if *precise {
                index
                    .files
                    .values()
                    .map(|r| r.calls.iter().filter(|c| c.resolved.is_none()).count())
                    .sum::<usize>() as u64
            } else {
                0
            };

            // Set total for unified progress bar
            let total_work = files_to_index + unresolved_calls;
            if total_work > 0 {
                progress.set_indexing_total(files_to_index);
                if *precise {
                    progress.set_lsp_total(unresolved_calls);
                }
            }

            // Index stale files
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

                        progress.indexing_file(rel_path);

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

            // LSP resolution if precise mode
            let has_any_resolved = index.calls().any(|c| c.resolved.is_some());
            if *precise && (updated > 0 || !has_any_resolved) {
                // Re-count after indexing (new calls may have been added)
                let new_unresolved: usize = index
                    .files
                    .values()
                    .map(|r| r.calls.iter().filter(|c| c.resolved.is_none()).count())
                    .sum();

                if new_unresolved > 0 {
                    progress.set_lsp_total(new_unresolved as u64);
                    let resolved = resolve_calls_with_lsp(&root, &mut index, &progress)?;
                    if resolved > 0 {
                        debug!("Resolved {} calls with LSP", resolved);
                    }
                }
            }

            let file_count = index.files.len();
            let def_count = index.definitions().count();
            let call_count = index.calls().count();
            let resolved_count = index.calls().filter(|c| c.resolved.is_some()).count();

            let summary = format!(
                "{} files, {} defs, {} calls ({} resolved)",
                file_count, def_count, call_count, resolved_count
            );
            progress.finish(&summary);

            save_index(&index, &root)?;
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

type CacheKey = (String, Option<String>, String);

fn resolve_calls_with_lsp(
    root: &Path,
    index: &mut Index,
    progress: &ProgressContext,
) -> Result<usize> {
    use glimpse::code::index::ResolvedCall;

    let unresolved_count: usize = index
        .files
        .values()
        .map(|r| r.calls.iter().filter(|c| c.resolved.is_none()).count())
        .sum();

    if unresolved_count == 0 {
        return Ok(0);
    }

    progress.lsp_warming("LSP");

    let rt = tokio::runtime::Runtime::new()?;
    let concurrency = 50;

    let (resolved, stats, cache_hits, cache_misses, timing) = rt.block_on(async {
        let mut resolver = AsyncLspResolver::new(root);
        let mut cache: HashMap<CacheKey, Option<ResolvedCall>> = HashMap::new();
        let mut total_resolved = 0usize;

        // Group calls by cache_key - only resolve ONE per unique key
        // calls_by_key: cache_key -> (representative call, list of (file_path, call_idx) to update)
        let mut calls_by_key: HashMap<CacheKey, (glimpse::code::index::Call, Vec<(PathBuf, usize)>)> = HashMap::new();

        for (file_path, record) in &index.files {
            for (call_idx, call) in record.calls.iter().enumerate() {
                if call.resolved.is_some() {
                    continue;
                }

                let ext = call.file.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
                let cache_key: CacheKey = (
                    call.callee.clone(),
                    call.qualifier.clone(),
                    ext,
                );

                calls_by_key
                    .entry(cache_key)
                    .or_insert_with(|| (call.clone(), Vec::new()))
                    .1
                    .push((file_path.clone(), call_idx));
            }
        }

        let unique_calls: Vec<_> = calls_by_key.keys().cloned().collect();
        let dedup_count = unique_calls.len();
        let total_call_count: usize = calls_by_key.values().map(|(_, locs)| locs.len()).sum();

        // Resolve only unique calls
        if !calls_by_key.is_empty() {
            let calls_to_resolve: Vec<_> = unique_calls.iter()
                .map(|k| &calls_by_key.get(k).unwrap().0)
                .collect();
            let skip_hover = true;
            let results = resolver
                .resolve_calls_batch(&calls_to_resolve, index, concurrency, skip_hover, |server, file, callee| {
                    progress.lsp_resolving(server, file, callee);
                })
                .await;

            for (batch_idx, resolved_call) in results {
                let cache_key = &unique_calls[batch_idx];
                cache.insert(cache_key.clone(), Some(resolved_call.clone()));

                if let Some((_call, locations)) = calls_by_key.get(cache_key) {
                    for (file_path, call_idx) in locations {
                        if let Some(record) = index.files.get_mut(file_path) {
                            if *call_idx < record.calls.len() {
                                record.calls[*call_idx].resolved = Some(resolved_call.clone());
                                total_resolved += 1;
                            }
                        }
                    }
                }
            }

            // Mark unresolved calls in cache as None
            for cache_key in &unique_calls {
                if !cache.contains_key(cache_key) {
                    cache.insert(cache_key.clone(), None);
                }
            }
        }

        let cache_hits = total_call_count.saturating_sub(dedup_count);
        let cache_misses = dedup_count;

        resolver.shutdown_all().await;
        let stats = resolver.stats().clone();
        let timing = resolver.timing_stats().to_string();
        (total_resolved, stats, cache_hits, cache_misses, timing)
    });

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

    eprintln!("\n{}", timing);

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
