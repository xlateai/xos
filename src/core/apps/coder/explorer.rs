#[derive(Debug, Clone)]
pub enum ExplorerItem {
    Folder(String),
    File(usize),
}

pub fn split_folder_file(path: &str) -> (String, String) {
    let normalized = path.replace('\\', "/");
    if let Some((folder, file)) = normalized.rsplit_once('/') {
        (folder.to_string(), file.to_string())
    } else {
        (String::new(), normalized)
    }
}

pub fn build_explorer_items(file_names: &[String]) -> Vec<ExplorerItem> {
    let mut folder_names: Vec<String> = file_names
        .iter()
        .map(|n| split_folder_file(n).0)
        .collect();
    folder_names.sort();
    folder_names.dedup();

    let mut items = Vec::new();
    for folder in folder_names {
        if !folder.is_empty() {
            items.push(ExplorerItem::Folder(folder.clone()));
        }

        let mut folder_files: Vec<(String, usize)> = file_names
            .iter()
            .enumerate()
            .filter_map(|(idx, name)| {
                let (f, base) = split_folder_file(name);
                (f == folder).then_some((base, idx))
            })
            .collect();
        folder_files.sort_by(|a, b| a.0.cmp(&b.0));

        for (_, idx) in folder_files {
            items.push(ExplorerItem::File(idx));
        }
    }
    items
}

