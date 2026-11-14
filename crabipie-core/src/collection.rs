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


/// First id that is free across the whole tree (ids start at 1).
pub fn next_free_id(items: &[CollectionItem]) -> usize {
    fn max(items: &[CollectionItem]) -> usize {
        items
            .iter()
            .map(|i| match i {
                CollectionItem::Folder(f) => f.id.max(max(&f.children)),
                CollectionItem::Request(r) => r.id,
            })
            .max()
            .unwrap_or(0)
    }
    max(items) + 1
}

pub fn find_name(items: &[CollectionItem], id: usize) -> Option<String> {
    for item in items {
        if item.id() == id {
            return Some(item.name().to_string());
        }
        if let CollectionItem::Folder(f) = item {
            if let Some(n) = find_name(&f.children, id) {
                return Some(n);
            }
        }
    }
    None
}

/// Every folder, depth-first, for a "save into" picker. Root comes first.
pub fn folder_options(items: &[CollectionItem]) -> Vec<(Option<usize>, String)> {
    fn walk(items: &[CollectionItem], depth: usize, out: &mut Vec<(Option<usize>, String)>) {
        for item in items {
            if let CollectionItem::Folder(f) = item {
                out.push((Some(f.id), format!("{}{}", "› ".repeat(depth), f.name)));
                walk(&f.children, depth + 1, out);
            }
        }
    }
    let mut out = vec![(None, "Root".to_string())];
    walk(items, 0, &mut out);
    out
}

/// Expand a folder by id (so a freshly inserted child is visible).
pub fn expand_item(items: &mut Vec<CollectionItem>, id: usize) -> bool {
    for item in items.iter_mut() {
        if let CollectionItem::Folder(f) = item {
            if f.id == id {
                f.expanded = true;
                return true;
            }
            if expand_item(&mut f.children, id) {
                return true;
            }
        }
    }
    false
}

fn reassign_ids(item: &mut CollectionItem, next: &mut usize) {
    match item {
        CollectionItem::Folder(f) => {
            f.id = *next;
            *next += 1;
            for c in f.children.iter_mut() {
                reassign_ids(c, next);
            }
        }
        CollectionItem::Request(r) => {
            r.id = *next;
            *next += 1;
        }
    }
}

/// Like `duplicate_item`, but gives the copy (and everything inside a copied
/// folder) fresh ids, so ids stay unique across the tree.
pub fn duplicate_item_fresh(items: &mut Vec<CollectionItem>, id: usize, next: &mut usize) -> bool {
    if let Some(pos) = items.iter().position(|i| i.id() == id) {
        let mut copy = items[pos].clone();
        reassign_ids(&mut copy, next);
        match &mut copy {
            CollectionItem::Folder(f) => f.name = format!("{} (copy)", f.name),
            CollectionItem::Request(r) => r.name = format!("{} (copy)", r.name),
        }
        items.insert(pos + 1, copy);
        return true;
    }
    for item in items.iter_mut() {
        if let CollectionItem::Folder(f) = item {
            if duplicate_item_fresh(&mut f.children, id, next) {
                return true;
            }
        }
    }
    false
}

/// Where a dragged item lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropAt {
    /// Last child of a folder (`None` = end of the root list).
    Into(Option<usize>),
    Before(usize),
    After(usize),
}

fn find_item(items: &[CollectionItem], id: usize) -> Option<&CollectionItem> {
    for item in items {
        if item.id() == id {
            return Some(item);
        }
        if let CollectionItem::Folder(f) = item {
            if let Some(found) = find_item(&f.children, id) {
                return Some(found);
            }
        }
    }
    None
}

fn contains_id(item: &CollectionItem, id: usize) -> bool {
    match item {
        CollectionItem::Folder(f) => f.id == id || f.children.iter().any(|c| contains_id(c, id)),
        CollectionItem::Request(r) => r.id == id,
    }
}

fn insert_rel(items: &mut Vec<CollectionItem>, target: usize, item: CollectionItem, after: bool) -> Result<(), CollectionItem> {
    if let Some(pos) = items.iter().position(|i| i.id() == target) {
        items.insert(pos + after as usize, item);
        return Ok(());
    }
    let mut item = item;
    for it in items.iter_mut() {
        if let CollectionItem::Folder(f) = it {
            match insert_rel(&mut f.children, target, item, after) {
                Ok(()) => return Ok(()),
                Err(back) => item = back,
            }
        }
    }
    Err(item)
}

/// Move an item (with everything inside it). Refuses to drop a folder into
/// itself or its own descendants; returns whether anything moved.
pub fn move_item(items: &mut Vec<CollectionItem>, id: usize, at: DropAt) -> bool {
    let target = match at {
        DropAt::Into(t) => t,
        DropAt::Before(t) | DropAt::After(t) => Some(t),
    };
    let Some(moving) = find_item(items, id) else { return false };
    if let Some(t) = target {
        if find_item(items, t).is_none() || contains_id(moving, t) {
            return false;
        }
    }
    if let DropAt::Into(Some(f)) = at {
        if !matches!(find_item(items, f), Some(CollectionItem::Folder(_))) {
            return false;
        }
    }
    let Some(item) = remove_item(items, id) else { return false };
    match at {
        DropAt::Into(parent) => {
            if let Some(f) = parent {
                expand_item(items, f);
            }
            insert_into(items, parent, item);
        }
        DropAt::Before(t) | DropAt::After(t) => {
            if let Err(item) = insert_rel(items, t, item, matches!(at, DropAt::After(_))) {
                items.push(item);
            }
        }
    }
    true
}

pub fn collapse_all(items: &mut Vec<CollectionItem>) {
    for item in items.iter_mut() {
        if let CollectionItem::Folder(f) = item {
            f.expanded = false;
            collapse_all(&mut f.children);
        }
    }
}
