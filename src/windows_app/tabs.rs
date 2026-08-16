//! Stable tab identity, ordering, shortcuts, and tab-strip hit testing.

mod collection;
mod selection;
mod shortcuts;
mod strip;

pub(super) use collection::{IdentifiedTab, RecentlyClosedTabs, TabCollection, TabId};
pub(super) use shortcuts::{BrowserShortcut, KeyModifiers, shortcut_for_key};
pub(super) use strip::{TabStripHit, TabStripLayout};

pub(super) const MAX_OPEN_TABS: usize = 16;
pub(super) const MAX_RECENTLY_CLOSED_TABS: usize = 10;
