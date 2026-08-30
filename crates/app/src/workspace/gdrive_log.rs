use std::path::Path;

use workbench_integration::PreprocessResult;

pub fn preprocess_start_message(
    processor: &str,
    source_csv: &Path,
    language_url: &str,
    metadata_csv: &Path,
    config_file: Option<&Path>,
) -> String {
    format!(
        "[INFO] Running {processor}...\n\
         [INFO] Source CSV: {}\n\
         [INFO] Language mapping URL: {language_url}\n\
         [INFO] Config: {}\n\
         [INFO] Expected output: {}",
        source_csv.display(),
        config_file
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "not supplied".to_string()),
        metadata_csv.display(),
    )
}

pub fn preprocess_success_messages(result: &PreprocessResult) -> Vec<String> {
    let details = &result.details;
    let mut lines = vec![
        format!(
            "[INFO] Processor finished: rows={}, cells_modified={}, validation_failures={}",
            details.processing_stats.total_rows,
            details.processing_stats.cells_modified,
            details.processing_stats.validation_failures
        ),
        format!("[INFO] New metadata CSV: {}", result.metadata_csv.display()),
    ];
    if let (Some(path), Some(stats)) = (
        details.items_output_path.as_ref(),
        details.items_stats.as_ref(),
    ) {
        lines.push(format!(
            "[INFO] Items CSV: {path} (items={}, unique_parents={}, skipped={})",
            stats.total_items, stats.unique_parents, stats.skipped_rows
        ));
    }
    lines
}

pub fn preprocess_error_message(err: &anyhow::Error) -> String {
    format!("[ERROR] Processor failed: {err:#}")
}
