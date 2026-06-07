use std::path::Path;

use workbench_integration::ProcessResult;

pub fn sheet_preprocess_start_message(
    node_id: &str,
    sheet_url: &str,
    language_url: &str,
    metadata_csv: &Path,
) -> String {
    format!(
        "[INFO] Running sheet preprocessor (--full, node={})…\n\
         [INFO] Sheet URL: {}\n\
         [INFO] Language mapping URL: {}\n\
         [INFO] Output: {} (and items CSV in the same folder)",
        node_id.trim(),
        sheet_url.trim(),
        language_url,
        metadata_csv.display(),
    )
}

pub fn sheet_preprocess_success_messages(res: &ProcessResult) -> Vec<String> {
    let mut lines = vec![
        format!(
            "[INFO] Sheet preprocessor finished: rows={}, cells_modified={}, validation_failures={}",
            res.processing_stats.total_rows,
            res.processing_stats.cells_modified,
            res.processing_stats.validation_failures
        ),
        format!("[INFO] Processed CSV: {}", res.processed_output_path),
    ];
    if let (Some(path), Some(stats)) = (res.items_output_path.as_ref(), res.items_stats.as_ref()) {
        lines.push(format!(
            "[INFO] Items CSV: {} (items={}, unique_parents={}, skipped={})",
            path, stats.total_items, stats.unique_parents, stats.skipped_rows
        ));
    }
    lines
}

pub fn sheet_preprocess_error_message(err: &anyhow::Error) -> String {
    format!("[ERROR] Sheet preprocessor failed: {:#}", err)
}
