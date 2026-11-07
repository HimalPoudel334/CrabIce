//! Tab management: lookup, add, close, select.

use crate::app::CrabiPie;
use crate::app::tab::{TabLoadState, TabState};

impl CrabiPie {
    pub(crate) fn current_tab(&self) -> Option<&TabState> {
        let slot = self.tabs.get(self.active_tab)?;
        match slot {
            TabLoadState::Loaded(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn current_tab_mut(&mut self) -> Option<&mut TabState> {
        let slot = self.tabs.get_mut(self.active_tab)?;
        match slot {
            TabLoadState::Loaded(state) => {
                state.dirty = true;
                Some(state)
            }
            _ => None,
        }
    }

    pub(crate) fn add_tab(&mut self) {
        let new_tab = TabLoadState::Loaded(Box::new(TabState::new(
            self.next_tab_id,
            self.active_environment_id,
        )));
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
        self.next_tab_id += 1;
    }

    pub(crate) fn close_tab(&mut self, index: usize) {
        if self.tabs.len() > 1 {
            self.tabs.remove(index);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            } else if self.active_tab > index {
                self.active_tab -= 1;
            }
        }
    }

    pub(crate) fn select_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
        }
    }
}
