//! `update` handlers for the **collections** messages.

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::app::tab::{TabLoadState, TabState};
use crate::domain::collection::{CollectionFolder, CollectionItem, CollectionRequest};
use crate::domain::collection;

pub(crate) fn handle(app: &mut CrabiPie, message: Message) -> iced::Task<Message> {
    match message {
        Message::CollectionLoaded(maybe_collection) => {
            if let Some(collection) = maybe_collection {
                app.collection = collection;
            }
            iced::Task::none()
        }
        Message::ToggleSidebar => {
            app.sidebar_open = !app.sidebar_open;
            iced::Task::none()
        }
        Message::SidebarItemSelected(id) => {
            app.sidebar_selected_id = Some(id);
            iced::Task::none()
        }
        Message::CollectionFolderAdd(parent_id) => {
            let id = app.next_collection_id();
            let folder = CollectionItem::Folder(CollectionFolder {
                id,
                name: "New Folder".to_string(),
                expanded: true,
                children: Vec::new(),
            });
            collection::insert_into(&mut app.collection.items, parent_id, folder);
            // immediately enter rename mode
            app.sidebar_editing_id = Some(id);
            app.sidebar_editing_name = "New Folder".to_string();
            app.collection_save_task()
        }
        Message::CollectionItemToggleExpand(id) => {
            collection::toggle_expand(&mut app.collection.items, id);
            iced::Task::none()
        }
        Message::CollectionRequestOpen(id) => {
            app.sidebar_selected_id = Some(id);
            if let Some(req) = collection::find_request(&app.collection.items, id) {
                let saved = req.saved_state.clone();
                let new_tab = TabLoadState::Loaded(Box::new(TabState::from_saved(saved)));
                app.tabs.push(new_tab);
                app.active_tab = app.tabs.len() - 1;
                app.next_tab_id += 1;
            }
            iced::Task::none()
        }
        Message::CollectionItemRename(id) => {
            // find current name
            fn find_name(items: &[CollectionItem], id: usize) -> Option<String> {
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
            if let Some(name) = find_name(&app.collection.items, id) {
                app.sidebar_editing_id = Some(id);
                app.sidebar_editing_name = name;
            }
            iced::Task::none()
        }
        Message::CollectionItemRenameInput(text) => {
            app.sidebar_editing_name = text;
            iced::Task::none()
        }
        Message::CollectionItemRenameConfirm(id) => {
            let new_name = app.sidebar_editing_name.trim().to_string();
            if !new_name.is_empty() {
                collection::rename_item(&mut app.collection.items, id, new_name);
            }
            app.sidebar_editing_id = None;
            app.sidebar_editing_name = String::new();
            app.collection_save_task()
        }
        Message::CollectionItemRenameCancel => {
            app.sidebar_editing_id = None;
            app.sidebar_editing_name = String::new();
            iced::Task::none()
        }
        Message::CollectionItemDelete(id) => {
            collection::remove_item(&mut app.collection.items, id);
            app.collection_save_task()
        }
        Message::CollectionItemDuplicate(id) => {
            let new_id = app.next_collection_id();
            collection::duplicate_item(&mut app.collection.items, id, new_id);
            app.collection_save_task()
        }
        Message::OpenSaveModal => {
            // Scope the borrow so it ends immediately after we get the title
            let title = {
                let Some(tab) = app.current_tab() else {
                    return iced::Task::none();
                };
                tab.title.clone()
            }; // The borrow of 'app' ends right here because 'tab' is gone

            // Now 'app' is free to be modified mutably
            app.save_modal_open = true;
            app.save_modal_name = title;
            app.save_modal_folder_id = None;

            iced::Task::none()
        }
        Message::SaveModalNameChanged(name) => {
            app.save_modal_name = name;
            iced::Task::none()
        }
        Message::SaveModalFolderSelected(folder_id) => {
            app.save_modal_folder_id = folder_id;
            iced::Task::none()
        }
        Message::SaveModalConfirm => {
            // 1. Get the data from the tab first (Immutable borrow phase)
            let (method, saved_state) = {
                let Some(tab) = app.current_tab() else {
                    return iced::Task::none();
                };
                (
                    tab.method.clone(),
                    tab.to_saved(&app.json_theme.to_string(), &app.app_theme.to_string()),
                )
            }; // The immutable borrow of 'app' ends here

            // 2. Now 'app' is free! You can now borrow it mutably.
            let id = app.next_collection_id(); // Works now!

            let req = CollectionItem::Request(CollectionRequest {
                id,
                name: app.save_modal_name.trim().to_string(),
                method,
                saved_state,
            });

            let folder_id = app.save_modal_folder_id;

            // Perform the insertion
            collection::insert_into(&mut app.collection.items, folder_id, req);

            // 3. Cleanup
            app.save_modal_open = false;
            app.save_modal_name = String::new();
            app.save_modal_folder_id = None;

            app.collection_save_task()
        }
        Message::SaveModalCancel => {
            app.save_modal_open = false;
            app.save_modal_name = String::new();
            app.save_modal_folder_id = None;
            iced::Task::none()
        }
        Message::CollectionSaved => iced::Task::none(),
        other => unreachable!("message routed to the wrong handler: {other:?}"),
    }
}
