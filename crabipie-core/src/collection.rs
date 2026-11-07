use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub name: String,
    pub items: Vec<CollectionItem>,
}

impl Collection {
    pub fn new() -> Self {
        Self {
            name: "My Collection".to_string(),
            items: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollectionItem {
    Folder(CollectionFolder),
    Request(CollectionRequest),
}

impl CollectionItem {
    pub fn id(&self) -> usize {
        match self {
            CollectionItem::Folder(f) => f.id,
            CollectionItem::Request(r) => r.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            CollectionItem::Folder(f) => &f.name,
            CollectionItem::Request(r) => &r.name,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionFolder {
    pub id: usize,
    pub name: String,
    pub expanded: bool,
    pub children: Vec<CollectionItem>, // enables infinite nesting
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionRequest {
    pub id: usize,
    pub name: String,
    pub method: HttpMethod,
    pub saved_state: SavedState,
}

pub fn collection_file_path() -> std::path::PathBuf {
    let base = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let dir = base.join(".crabipie");
    std::fs::create_dir_all(&dir).ok();
    dir.join("collection.json")
}

pub async fn save_collection(collection: Collection) {
    if let Ok(json) = serde_json::to_string_pretty(&collection) {
        tokio::fs::write(collection_file_path(), json).await.ok();
    }
}

pub async fn load_collection() -> Option<Collection> {
    let bytes = tokio::fs::read(collection_file_path()).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

// ── tree operations (ids are unique across the whole tree) ──

/// Find and remove an item by id, returning it
pub fn remove_item(
    items: &mut Vec<CollectionItem>,
    id: usize,
) -> Option<CollectionItem> {
    if let Some(pos) = items.iter().position(|i| i.id() == id) {
        return Some(items.remove(pos));
    }
    for item in items.iter_mut() {
        if let CollectionItem::Folder(f) = item {
            if let Some(found) = remove_item(&mut f.children, id) {
                return Some(found);
            }
        }
    }
    None
}

// Find a request by id across all nested items
pub fn find_request(items: &[CollectionItem], id: usize) -> Option<&CollectionRequest> {
    for item in items {
        match item {
            CollectionItem::Request(r) if r.id == id => return Some(r),
            CollectionItem::Folder(f) => {
                if let Some(found) = find_request(&f.children, id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

// Insert item into a folder by id, or root if None
pub fn insert_into(
    items: &mut Vec<CollectionItem>,
    target_folder_id: Option<usize>,
    item: CollectionItem,
) -> bool {
    // return true if inserted
    match target_folder_id {
        None => {
            items.push(item);
            true
        }
        Some(folder_id) => {
            for existing in items.iter_mut() {
                if let CollectionItem::Folder(f) = existing {
                    if f.id == folder_id {
                        f.children.push(item);
                        return true;
                    }
                    if insert_into(
                        &mut f.children,
                        Some(folder_id),
                        item.clone(),
                    ) {
                        return true;
                    }
                }
            }
            false
        }
    }
}

pub fn toggle_expand(items: &mut Vec<CollectionItem>, id: usize) {
    for item in items.iter_mut() {
        if let CollectionItem::Folder(f) = item {
            if f.id == id {
                f.expanded = !f.expanded;
                return;
            }
            toggle_expand(&mut f.children, id);
        }
    }
}

pub fn rename_item(items: &mut Vec<CollectionItem>, id: usize, new_name: String) {
    for item in items.iter_mut() {
        match item {
            CollectionItem::Folder(f) if f.id == id => {
                f.name = new_name;
                return;
            }
            CollectionItem::Request(r) if r.id == id => {
                r.name = new_name;
                return;
            }
            CollectionItem::Folder(f) => {
                rename_item(&mut f.children, id, new_name.clone());
            }
            _ => {}
        }
    }
}

pub fn duplicate_item(items: &mut Vec<CollectionItem>, id: usize, new_id: usize) {
    // find position and parent
    for i in 0..items.len() {
        match &items[i] {
            CollectionItem::Request(r) if r.id == id => {
                let mut cloned = r.clone();
                cloned.id = new_id;
                cloned.name = format!("{} (copy)", cloned.name);
                items.insert(i + 1, CollectionItem::Request(cloned));
                return;
            }
            CollectionItem::Folder(f) if f.id == id => {
                let mut cloned = f.clone();
                cloned.id = new_id;
                cloned.name = format!("{} (copy)", cloned.name);
                items.insert(i + 1, CollectionItem::Folder(cloned));
                return;
            }
            CollectionItem::Folder(_) => {
                if let CollectionItem::Folder(f) = &mut items[i] {
                    duplicate_item(&mut f.children, id, new_id);
                }
            }
            _ => {}
        }
    }
}

