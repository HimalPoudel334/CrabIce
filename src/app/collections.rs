//! Collection bookkeeping that needs app state (ids, save task).

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::domain::collection::save_collection;

impl CrabiPie {
    pub(crate) fn collection_save_task(&self) -> iced::Task<Message> {
        let collection = self.collection.clone();
        iced::Task::perform(save_collection(collection), |_| Message::CollectionSaved)
    }

    pub(crate) fn next_collection_id(&mut self) -> usize {
        let id = self.next_collection_id;
        self.next_collection_id += 1;
        id
    }
}
